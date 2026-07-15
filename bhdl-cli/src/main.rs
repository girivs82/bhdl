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

mod value_deriver;
mod mech_check;
use anyhow::{Result, Context};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use log::info;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};
use bhdl_analyzer::{analyze, documentation::{generate_documentation, DocumentationOptions, OutputFormat}};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_schematic;
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

    /// Fail the run when unwaived ERC/DRC findings reach this severity:
    /// `critical`, `error`, or `warning`. Default: report-only.
    #[arg(long, value_name = "LEVEL")]
    erc_fail_on: Option<String>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Select a board SKU variant declared via `variant <Name> { ... }`
    /// in the .bhdl file. Patches the post-expansion netlist with the
    /// variant's value overrides and DNP marks before any consuming
    /// subcommand runs (bom, spice, layout, …). When the board declares
    /// variants but `--sku` is omitted, those subcommands error out
    /// with a list of declared variants. See
    /// docs/spec/Board_SKU_Variants.md.
    #[arg(long, value_name = "NAME")]
    sku: Option<String>,

    /// Path to the project manifest (`bhdl.toml`) declaring library
    /// dependencies. When omitted, BHDL discovers one by walking up
    /// from the input file's directory (Cargo-style). Only needed when
    /// the board imports from a non-`bhdl-stdlib` library.
    /// See docs/spec/Library_Resolution.md.
    #[arg(long, value_name = "FILE")]
    manifest: Option<PathBuf>,

    /// Library search root for resolving declared (name-only)
    /// dependencies — repeatable, highest precedence first. Mirrors a
    /// C compiler's `-I`. Authoritative over `$BHDL_LIB_PATH`. Point
    /// these at proprietary/internal library roots.
    #[arg(short = 'I', long = "lib-path", value_name = "DIR")]
    lib_path: Vec<PathBuf>,

    /// Regenerate `bhdl.lock` from the current libraries even if it
    /// exists and has drifted. Use this when you intentionally bump or
    /// edit a dependency. Without it, a drifted lock is a hard error.
    #[arg(long)]
    update_lock: bool,

    /// CI mode: require `bhdl.lock` to exist and match exactly; never
    /// generate or update it. Errors if the lock is missing or drifted.
    #[arg(long)]
    locked: bool,

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

    /// Emit a frozen structural netlist — the immutable "as-fabbed"
    /// record (resolved components + flat connectivity, stamped with
    /// the toolchain version + library lock). Stable, versioned schema;
    /// archive it alongside the board. See docs/spec/Library_Resolution.md.
    Freeze {
        /// Output file (JSON). Defaults to `<input>.frozen.json`.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Generate interactive schematic visualization (HTML)
    Visualize {
        /// Output HTML file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output raw SchematicData JSON instead of HTML
        #[arg(long)]
        json: bool,

        /// Also write the V4 idiom-composed SVG schematic to this path
        #[arg(long, value_name = "SVG")]
        svg_v4: Option<String>,

        /// Bind the sheet tree into a print-ready multipage HTML document
        /// (index page + title blocks; print to PDF from any browser)
        #[arg(long, value_name = "HTML")]
        binder: Option<String>,
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
    
    /// Time-domain simulation of scheduled IBIS buffer edges
    /// (`ibis_wave_<PIN>="rise@2n fall@10n"` instance directives)
    Transient {
        /// Output CSV file for the probe traces (time + one column per net)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Extra nets to probe (driven pins' nets are always probed)
        #[arg(long)]
        probe: Vec<String>,

        /// Simulation duration, e.g. "50n" (default: latest edge horizon + 50%)
        #[arg(long)]
        duration: Option<String>,

        /// Fixed timestep, e.g. "0.05n" (default: duration / 400)
        #[arg(long)]
        timestep: Option<String>,

        /// Sweep all three silicon corners (typ/min/max) instead of the
        /// stanza-declared corner; corners without table data are skipped
        #[arg(long)]
        corners: bool,
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

    /// Run PCB place & route and export KiCad PCB
    Layout {
        /// Output .kicad_pcb file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Number of placement trials (best-of-N selection)
        #[arg(short, long, default_value = "3")]
        trials: usize,

        /// Maximum placement iterations per trial
        #[arg(long, default_value = "600")]
        max_iterations: usize,

        /// Base RNG seed — trial k runs at seed+k. Same seed + same
        /// input = byte-identical board, so changes can be A/B'd
        /// instead of argued with the dice
        #[arg(long, default_value = "42")]
        seed: u64,

        /// Also generate interactive HTML board visualization
        #[arg(long)]
        html: bool,
    },

    /// Transcribe a MCAD DXF (positional FILE) into layout-block
    /// mechanical statements: outline, mounting_hole, mech_check.
    /// One-time onboarding — paste the output into the board's layout
    /// block; thereafter the .bhdl is authoritative and mech_check
    /// catches drift on every build.
    MechImport {
        /// DXF is Y-up (typical MCAD export) — mirror into the board's
        /// Y-down frame
        #[arg(long)]
        flip_y: bool,
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

    /// Generate a manufacturing Bill of Materials from the synthesized
    /// netlist. Walks every physical-component instance, reads the
    /// canonical SKU attributes (manufacturer, mpn, package,
    /// distributor PNs) declared on entities and instance overrides,
    /// groups identical parts, and emits either Markdown or CSV.
    ///
    /// When the board declares variants, pass `--sku <Name>` at the
    /// top level to select which one to generate the BOM for.
    Bom {
        /// Output file path. Defaults to stdout when omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format. `markdown` for human-readable tables,
        /// `csv` for assembly-house parts lists.
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Supply-chain optimization profile for resolving real MPNs:
        /// `precision` (exact E-series value), `grade` (precision path:
        /// exact value + tight tolerance + low drift, for feedback /
        /// measurement), `cost` (cheapest to assemble), `availability`
        /// (max stock / min lead), or `balanced` (default). Overridable
        /// per part via a `supply_profile` attribute, or per net via
        /// --supply-net.
        #[arg(long, value_name = "PROFILE")]
        supply_profile: Option<String>,

        /// Target build quantity — selects the price tier and weights
        /// stock headroom. Defaults to 1.
        #[arg(long, value_name = "N")]
        supply_qty: Option<u64>,

        /// Per-net supply profile override, `NET=PROFILE` (repeatable),
        /// e.g. `--supply-net FB=precision --supply-net VCC=cost`.
        #[arg(long = "supply-net", value_name = "NET=PROFILE")]
        supply_net: Vec<String>,

        /// Run a GLACIER DC solve and derive each passive's stress (cap
        /// voltage, resistor power, inductor current) from the simulated
        /// operating point, instead of declared-rail-voltage-only. The
        /// inductor current gate is then fed by simulation rather than the
        /// recipe's closed-form seed. Best-effort: if the solve fails the
        /// BOM still generates from declared stress.
        #[arg(long)]
        simulate: bool,
    },

    /// List the SKU variants declared by the board. Prints "default"
    /// when no `variant` blocks are present.
    /// Full synthesis report (Markdown on stdout): requirements,
    /// generated instantiation, design values, simulation/stress results,
    /// sign-off incl. requirement rows, and the BOM with MPNs.
    Report {},
    ListSkus,
}

#[tokio::main]
async fn main() -> Result<()> {
    // `bhdl vendor <status|install ...>` is board-independent — intercept
    // before clap, whose grammar requires an input FILE for every other
    // command.
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("vendor") {
            return cmd_vendor(&args[2..]);
        }
    }
    let cli = Cli::parse();

    // Install the process-wide import search context so direct-relative
    // import strings ("bhdl-stdlib/…") resolve from any working
    // directory: input file's dir, then -I roots, then $BHDL_LIB_PATH,
    // then cwd (legacy).
    bhdl_common::import_search::set_search_context(
        cli.input.parent().map(|p| p.to_path_buf()),
        &cli.lib_path,
    );

    // Initialize logging: --verbose defaults to debug, otherwise info;
    // RUST_LOG (when set) takes precedence over both.
    let default_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .init();
    
    // Configure Cargo-style library resolution (proprietary / external
    // stdlibs). Activates only when the board opts in — a `bhdl.toml`
    // is found (explicit --manifest, else discovered by walking up from
    // the input file) OR `-I`/`$BHDL_LIB_PATH` is supplied. Otherwise
    // imports keep legacy literal-path behaviour (stdlib-only boards
    // need no manifest). See docs/spec/Library_Resolution.md.
    // Captured for `freeze` provenance: the resolved library lock
    // (name + exact version + content hash) this build used.
    let mut frozen_libraries: Vec<bhdl_common::library::LockedLibrary> = Vec::new();
    if let Some(resolver) = build_library_resolver(&cli)? {
        if let Ok(lock) = resolver.compute_lockfile() {
            frozen_libraries = lock.libraries;
        }
        bhdl_synthesizer::set_global_library_resolver(resolver);
    }

    // mech-import takes a DXF as its positional input, not BHDL —
    // dispatch before the source-file read/parse pipeline.
    if let Some(Commands::MechImport { flip_y }) = &cli.command {
        let dxf = mech_check::parse_dxf(&cli.input)?;
        let name = cli
            .input
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cli.input.display().to_string());
        print!("{}", mech_check::render_import(&dxf, &name, *flip_y)?);
        return Ok(());
    }

    // Read input file
    let input_content = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read file: {}", cli.input.display()))?;

    // Power-supply synthesis (docs/spec/Power_Supply_Synthesis.md): desugar
    // any `supply` requirement statements into the equivalent hand-written
    // instantiation + wiring BEFORE the main parse, so every downstream pass
    // sees plain BHDL. Prints the generated text — it is the report's
    // "winner and instantiation" section, and the user should see exactly
    // what their requirement compiled to.
    let (input_content, supply_syntheses) =
        match bhdl_synthesizer::supply_synthesis::desugar_supplies(
            &input_content,
            &bhdl_common::import_search::locate_dir("bhdl-stdlib")
                .unwrap_or_else(|| PathBuf::from("bhdl-stdlib")),
        ) {
            Ok(Some(d)) => {
                for su in &d.supplies {
                    println!(
                        "{} supply @{} from @{} → {} ({}){}",
                        "⚡".yellow(),
                        su.target_rail,
                        su.source_rail,
                        su.part,
                        su.instance,
                        if su.survey.is_empty() {
                            " [using: explicit]".to_string()
                        } else {
                            let n = su.survey.len();
                            let pass = su.survey.iter().filter(|c| c.loss_w.is_some()).count();
                            format!(" [chosen from {n} candidates, {pass} passed all gates]")
                        }
                    );
                    for line in su.generated.lines() {
                        println!("    {}", line.trim_start());
                    }
                }
                (d.source, d.supplies)
            }
            Ok(None) => (input_content, Vec::new()),
            Err(e) => {
                eprintln!("{} {e:#}", "supply synthesis error:".red().bold());
                std::process::exit(1);
            }
        };

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
        // Dispatched before the parse pipeline above.
        Some(Commands::MechImport { .. }) => unreachable!("handled pre-parse"),
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

        Some(Commands::Freeze { output }) => {
            run_freeze(&source_file, &cli.input, output, frozen_libraries).await?;
        }
        
        Some(Commands::Visualize { output, json, svg_v4, binder }) => {
            run_visualization(&source_file, output, json, &cli.input, svg_v4.as_deref(), binder.as_deref()).await?;
        }
        
        Some(Commands::Spice { analysis, output, use_metadata }) => {
            run_spice(&source_file, &analysis, output, use_metadata, cli.sku.as_deref()).await?;
        }
        
        Some(Commands::Transient { output, probe, duration, timestep, corners }) => {
            cmd_transient(&source_file, &cli.input, output, probe, duration, timestep, corners).await?;
        }
        Some(Commands::Pipeline { output_dir, no_viz, no_spice }) => {
            run_pipeline(&source_file, &cli.input, output_dir, no_viz, no_spice).await?;
        }
        
        Some(Commands::Simulate { testbench, output, format, verbose: _verbose }) => {
            run_simulation(&source_file, testbench, output, &format).await?;
        }

        Some(Commands::Layout { output, trials, max_iterations, html, seed }) => {
            run_layout(&source_file, output, trials, max_iterations, html, &cli.input, seed).await?;
        }

        Some(Commands::Doc { output, bom_only, budget_only, no_tree, no_patterns }) => {
            cmd_doc(&source_file, output, bom_only, budget_only, no_tree, no_patterns).await?;
        }

        Some(Commands::Bom { output, format, supply_profile, supply_qty, supply_net, simulate }) => {
            cmd_bom(
                &source_file,
                &cli.input,
                output,
                &format,
                cli.sku.as_deref(),
                supply_profile,
                supply_qty,
                supply_net,
                cli.update_lock,
                cli.locked,
                simulate,
            )
            .await?;
        }

        Some(Commands::ListSkus) => {
            cmd_list_skus(&source_file).await?;
        }

        Some(Commands::Report {}) => {
            // Synthesis report skeleton (Power_Supply_Synthesis.md §4):
            // Markdown on stdout — requirements + generated instantiation,
            // then the full bom+simulate output (sizing, stress, sign-off
            // incl. requirement rows, BOM w/ MPNs). Redirect to a file for
            // the document form; single-file -o capture arrives with the
            // report refactor, and sections 2–4 (topology math + candidate
            // survey) arrive with the S2 chooser.
            println!("# Synthesis report — {}\n", cli.input.display());
            if supply_syntheses.is_empty() {
                println!("_No `supply` requirement statements on this board — \
                          sections below cover the hand-instantiated design._\n");
            }
            for su in &supply_syntheses {
                println!("## Requirement: @{} from @{}\n", su.target_rail, su.source_rail);
                println!("| Spec | Value |");
                println!("|---|---|");
                for (k, v) in &su.specs {
                    println!("| {k} | {v} |");
                }
                if !su.survey.is_empty() {
                    println!("\n### Candidate survey (S2 chooser)\n");
                    println!("| Part | Verdict | Est. loss | Support | IC price | Support cost | Total | MPN (LCSC) |");
                    println!("|---|---|---|---|---|---|---|---|");
                    for c in &su.survey {
                        let verdict = if c.chosen {
                            "**CHOSEN**".to_string()
                        } else if c.loss_w.is_some() {
                            "pass".to_string()
                        } else {
                            let (g, d, _) = c
                                .gates
                                .iter()
                                .find(|(_, _, ok)| !ok)
                                .cloned()
                                .unwrap_or_default();
                            format!("REJECT ({g}: {d})")
                        };
                        let loss = c
                            .loss_w
                            .map(|w| format!("{w:.2}W"))
                            .unwrap_or_else(|| "—".into());
                        let price = c
                            .ic_price
                            .map(|p| format!("${p:.3}"))
                            .unwrap_or_else(|| "—".into());
                        let mpn = match (&c.ic_mpn, &c.ic_sku) {
                            (Some(m), Some(k)) => format!("{m} ({k})"),
                            _ => "—".into(),
                        };
                        let sup = match (c.support_cost, c.unpriced_parts) {
                            (Some(sc), 0) => format!("${sc:.3}"),
                            (Some(sc), n) => format!("${sc:.3} (+{n} unpriced)"),
                            (None, _) => "—".into(),
                        };
                        let total = match (c.ic_price, c.support_cost) {
                            (Some(a), Some(b)) => format!("${:.3}", a + b),
                            _ => "—".into(),
                        };
                        println!(
                            "| {} | {} | {} | {} | {} | {} | {} | {} |",
                            c.part, verdict, loss, c.support_parts, price, sup, total, mpn
                        );
                    }
                    println!("\n#### Per-candidate gate detail\n");
                    for c in &su.survey {
                        println!("**{}**{}", c.part, if c.chosen { " ← chosen (lowest estimated loss)" } else { "" });
                        for (g, d, ok) in &c.gates {
                            println!("- {} {g}: {d}", if *ok { "✔" } else { "✘" });
                        }
                        println!();
                    }
                } else if su.specs.iter().any(|(k, _)| k == "using") {
                    println!("\n_Part named explicitly (`using:`) — engineer override; no candidate survey run._");
                }
                for (title, xl, yl, pts, note) in &su.curves {
                    println!("\n### {title}\n");
                    // Inline SVG chart (GFM renders it); the table below
                    // keeps the exact numbers at ~6 sample points.
                    let svg = bhdl_synthesizer::supply_synthesis::curve_svg(title, xl, yl, pts);
                    if !svg.is_empty() {
                        println!("{svg}\n");
                    }
                    println!("| {xl} | {yl} |");
                    println!("|---|---|");
                    let step = (pts.len() / 5).max(1);
                    for (x, y) in pts.iter().step_by(step) {
                        println!("| {x:.3} | {y:.1} |");
                    }
                    println!("\n_{note}_");
                }
                println!("\n### Instantiation (using: {})\n", su.part);
                println!("```bhdl");
                if !su.import_line.is_empty() {
                    println!("{}", su.import_line);
                }
                println!("{}", su.generated);
                println!("```\n");
            }
            // S4c: the supply tree implies the power-up order — a rail
            // cannot come up before its source. Derived purely from the
            // supply edges (declared design facts, no timing guesses).
            if supply_syntheses.len() > 1
                || supply_syntheses
                    .first()
                    .map(|s| supply_syntheses.iter().any(|o| o.source_rail == s.target_rail))
                    .unwrap_or(false)
            {
                println!("## Power-up order (from the supply tree)\n");
                let targets: std::collections::HashSet<_> =
                    supply_syntheses.iter().map(|s| s.target_rail.clone()).collect();
                let mut level: Vec<String> = supply_syntheses
                    .iter()
                    .filter(|s| !targets.contains(&s.source_rail))
                    .map(|s| s.source_rail.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                level.sort();
                let mut stage = 1usize;
                let mut seen: std::collections::HashSet<String> = level.iter().cloned().collect();
                while !level.is_empty() {
                    println!("{stage}. {}", level.join(", "));
                    let mut next: Vec<String> = supply_syntheses
                        .iter()
                        .filter(|s| level.contains(&s.source_rail) && !seen.contains(&s.target_rail))
                        .map(|s| s.target_rail.clone())
                        .collect();
                    next.sort();
                    next.dedup();
                    for r in &next {
                        seen.insert(r.clone());
                    }
                    level = next;
                    stage += 1;
                }
                println!(
                    "\n_A rail is listed after its source; sequencing hardware \
                     (supervisors, EN daisy-chains) is the designer's to add — \
                     this section states the ORDER the tree implies._\n"
                );
            }

            println!("## Design, simulation, sign-off and BOM\n");
            cmd_bom(
                &source_file,
                &cli.input,
                None,
                "markdown",
                cli.sku.as_deref(),
                None,
                None,
                Vec::new(),
                cli.update_lock,
                cli.locked,
                true,
            )
            .await?;
        }

        Some(Commands::Intents { show_hints, show_rules, filter, format }) => {
            run_intents_analysis(&source_file, show_hints, show_rules, filter, &format).await?;
        }
    }

    // ERC gate: fail the run when unwaived findings reached the requested
    // severity (waived findings never count — they are recorded decisions).
    if let Some(level) = &cli.erc_fail_on {
        use bhdl_synthesizer::design_rule_checker::{
            ERC_GATE_CRITICAL, ERC_GATE_ERRORS, ERC_GATE_WARNINGS,
        };
        use std::sync::atomic::Ordering;
        let (c, e, w) = (
            ERC_GATE_CRITICAL.load(Ordering::Relaxed),
            ERC_GATE_ERRORS.load(Ordering::Relaxed),
            ERC_GATE_WARNINGS.load(Ordering::Relaxed),
        );
        let gate = match level.as_str() {
            "critical" => c,
            "error" => c + e,
            "warning" => c + e + w,
            other => {
                eprintln!("--erc-fail-on: unknown level '{other}' (critical|error|warning)");
                std::process::exit(2);
            }
        };
        if gate > 0 {
            eprintln!(
                "{} ERC gate: {gate} unwaived finding(s) at or above '{level}' \
                 ({c} critical, {e} errors, {w} warnings) — failing the build",
                "✖".red().bold()
            );
            std::process::exit(3);
        }
    }

    Ok(())
}

/// Build the Cargo-style library resolver from CLI flags + environment.
///
/// Returns `Some(resolver)` when library resolution should activate:
/// a `bhdl.toml` is found (explicit `--manifest`, else discovered by
/// walking up from the input file's directory) OR `-I`/`--lib-path`
/// roots were given. Returns `None` for the pure stdlib-only case
/// (no manifest, no lib paths) so imports keep their legacy
/// literal-path behaviour and nothing changes for existing boards.
fn build_library_resolver(
    cli: &Cli,
) -> Result<Option<bhdl_common::library::LibraryResolver>> {
    use bhdl_common::library::{discover_project_manifest, LibraryResolver};

    // Locate the manifest: explicit flag, else walk up from the input dir.
    let manifest_path = match &cli.manifest {
        Some(p) => Some(p.clone()),
        None => {
            let start = cli.input.parent().unwrap_or_else(|| std::path::Path::new("."));
            discover_project_manifest(start)
        }
    };

    let env_lib_path = std::env::var("BHDL_LIB_PATH").ok();

    // Activate only when the user opted in.
    if manifest_path.is_none() && cli.lib_path.is_empty() && env_lib_path.is_none() {
        return Ok(None);
    }

    if cli.verbose {
        if let Some(m) = &manifest_path {
            eprintln!("library resolver: manifest {}", m.display());
        }
        for r in &cli.lib_path {
            eprintln!("library resolver: -I {}", r.display());
        }
    }

    let resolver = LibraryResolver::new(
        manifest_path.as_deref(),
        &cli.lib_path,
        env_lib_path.as_deref(),
        None, // bundled stdlib falls back to literal `bhdl-stdlib/…`
    )?;

    // Lockfile gate (next to the manifest). Pins exact versions +
    // content hashes so a rebuild reproduces the same libraries or
    // fails loudly — never silently substitutes a changed recipe.
    if let Some(mp) = &manifest_path {
        enforce_lockfile(&resolver, mp, cli.update_lock, cli.locked, cli.verbose)?;
    } else if cli.locked {
        anyhow::bail!("--locked requires a bhdl.toml + bhdl.lock, but no manifest was found");
    }

    Ok(Some(resolver))
}

/// Generate, verify, or update `bhdl.lock` (sibling of the manifest).
///
///   - missing lock, default      → generate it (record exact versions + hashes)
///   - missing lock, --locked     → error (CI must build against a committed lock)
///   - present lock, matches      → ok
///   - present lock, drifted      → error (loud) unless --update-lock
///   - --update-lock              → regenerate regardless
fn enforce_lockfile(
    resolver: &bhdl_common::library::LibraryResolver,
    manifest_path: &std::path::Path,
    update_lock: bool,
    locked: bool,
    verbose: bool,
) -> Result<()> {
    use bhdl_common::library::Lockfile;

    let lock_path = manifest_path.with_file_name("bhdl.lock");
    let mut current = resolver.compute_lockfile()?;

    // Preserve any supply-chain part pins — they live in the same lockfile
    // but are owned by the BOM path, not the library resolver. Without this,
    // every library-lock (re)write would wipe the part section.
    if lock_path.is_file() {
        if let Ok(stored) = Lockfile::load(&lock_path) {
            current.parts = stored.parts;
        }
    }

    // Nothing declared → no lock needed.
    if current.libraries.is_empty() {
        return Ok(());
    }

    if update_lock {
        current.save(&lock_path)?;
        if verbose { eprintln!("library lock: updated {}", lock_path.display()); }
        return Ok(());
    }

    if lock_path.is_file() {
        let stored = Lockfile::load(&lock_path)?;
        let drifts = stored.diff(&current);
        if !drifts.is_empty() {
            eprintln!("{}", "Library lock drift — refusing to build:".red().bold());
            for d in &drifts {
                eprintln!("  {} {}", "•".red(), d);
            }
            anyhow::bail!(
                "bhdl.lock no longer matches the resolved libraries; \
                 restore the locked libraries or pass --update-lock if the change is intended"
            );
        }
        if verbose { eprintln!("library lock: verified {} ({} libs)", lock_path.display(), current.libraries.len()); }
    } else if locked {
        anyhow::bail!(
            "--locked was given but {} does not exist; commit a lockfile first (run without --locked to generate one)",
            lock_path.display()
        );
    } else {
        current.save(&lock_path)?;
        if verbose { eprintln!("library lock: generated {}", lock_path.display()); }
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

/// Refuse to build when the analyzer flagged a constructor argument that
/// binds to no declared parameter (E0402) or a value outside a parameter's
/// declared allowed set (E0403). Both used to pass silently; the user chose
/// to make them hard errors, so every netlist-producing command aborts
/// before synthesis with the offending args listed.
fn gate_constructor_args(analysis: &bhdl_analyzer::AnalysisResult) -> Result<()> {
    let bad: Vec<&bhdl_analyzer::Diagnostic> = analysis
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                bhdl_common::DiagnosticKind::UnknownConstructorArg { .. }
                    | bhdl_common::DiagnosticKind::ParameterValueNotAllowed { .. }
            )
        })
        .collect();
    if bad.is_empty() {
        return Ok(());
    }
    eprintln!(
        "{}",
        format!("Refusing to build: {} constructor argument error(s)", bad.len())
            .red()
            .bold()
    );
    for d in &bad {
        eprintln!("  {} {} [{}]", "•".red(), d.message, d.code);
    }
    anyhow::bail!("constructor argument errors (E0402/E0403)");
}

async fn run_synthesis(source_file: &SourceFile, output: Option<PathBuf>, format: &str) -> Result<()> {
    // First run analysis
    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;

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

/// Emit the frozen structural netlist — the immutable as-fabbed record.
async fn run_freeze(
    source_file: &SourceFile,
    source_path: &Path,
    output: Option<PathBuf>,
    libraries: Vec<bhdl_common::library::LockedLibrary>,
) -> Result<()> {
    use bhdl_synthesizer::freeze::{freeze_netlist, Provenance};

    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;
    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(source_path.with_extension("bhdl.refdes"));
    let netlist = generator
        .generate_from_ast_and_analysis(source_file, &analysis)
        .await
        .context("Failed to synthesize netlist for freeze")?;

    let generated_at = humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string();
    let provenance = Provenance {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        source: source_path.display().to_string(),
        generated_at,
        libraries,
    };

    let frozen = freeze_netlist(&netlist, provenance);
    let json = serde_json::to_string_pretty(&frozen)?;

    let out = output.unwrap_or_else(|| {
        let mut p = source_path.to_path_buf();
        p.set_extension("frozen.json");
        p
    });
    fs::write(&out, &json).with_context(|| format!("writing {}", out.display()))?;

    println!("{}", "✓ Frozen netlist written".green().bold());
    println!("  Components: {}", frozen.components.len());
    println!("  Nets:       {}", frozen.nets.len());
    println!("  Libraries:  {}", frozen.provenance.libraries.len());
    println!("  Output:     {}", out.display());
    Ok(())
}

async fn run_visualization(source_file: &SourceFile, output: Option<PathBuf>, json_output: bool, source_path: &Path, svg_v4: Option<&str>, binder: Option<&str>) -> Result<()> {
    // Run full pipeline to get netlist
    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;

    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(source_path.with_extension("bhdl.refdes"));
    let mut netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;

    // Stamp intent attributes from FlowTracker onto netlist instances
    // (bridges analyzer intents → synthesizer attributes for downstream passes)
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        bhdl_synthesizer::intent_attribute_stamper::stamp_intent_attributes(&mut netlist, flow_tracker);
    }

    // Expand entity instances via expansion { } blocks (new declarative approach)
    // Runs BEFORE legacy vpin expander so it takes priority
    let recipe_results = bhdl_synthesizer::expansion_interpreter::expand_entity_instances_with_designs(
        &mut netlist,
        &analysis.expansion_recipes,
        &analysis.design_recipes, &analysis.entity_attribute_index,
        &analysis.entity_param_names,
        &analysis.entity_attr_param_refs,
    );
    if !recipe_results.is_empty() {
        println!("  {} expansion blocks applied for {} entity instance(s)",
            "✓".green(), recipe_results.len());
        for r in &recipe_results {
            println!("    {} {} → {} child instance(s)",
                "→".cyan(), r.parent_instance, r.child_instances.len());
        }
    }

    // Snap computed passive values to catalog E-series before sim/viz.
    snap_catalog_values(&mut netlist);
    run_value_derivation(&mut netlist, &analysis, source_path)?;

    // Run GLACIER DC simulation for voltage/current annotation
    let sim_annotations = {
        let mut converter = NetlistToSpiceConverter::new();
        // §5 device-model surface: without the evaluated model overrides the
        // sheet's DC solve never sees vendor-authored sources OR loads — an
        // MCU's declared draw would be invisible and every rail current a
        // placeholder.
        converter.set_model_overrides(bhdl_synthesizer::model_evaluator::evaluate_model_overrides(
            &netlist,
            &analysis.model_recipes,
            &analysis.entity_attribute_index,
        ));
        // §5 vendor-model form #1: IBIS references, resolved against the
        // board source's directory.
        converter.set_ibis_models(
            analysis.model_recipes.iter()
                .filter_map(|(e, r)| (!r.ibis.is_empty()).then(|| (e.clone(), r.ibis.clone())))
                .collect(),
            source_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        );
        match converter.convert(&netlist) {
            Ok(circuit) => {
                // Input-draw fixpoint: regulators pull their efficiency-
                // derived input current from their SOLVED output current,
                // so cascade rails carry real numbers. Annotate against
                // the FINAL circuit (it holds the _in_draw branches).
                match bhdl_spice::input_draw::solve_dc_with_input_draws(circuit, &regulator_hints(&netlist)) {
                    Ok((result, circuit_ref)) => {
                        info!("GLACIER DC simulation converged in {} iterations (error: {:.2e})",
                              result.iterations, result.final_error);
                        let mut ann = build_simulation_annotations(&result, &circuit_ref);
                        ann.port_currents = compute_port_currents(&result, &circuit_ref, &netlist);
                        ann.stimulus = run_chain_stimulus(&netlist, &circuit_ref);
                        // Corner sweep: typ (with settle extension) sets
                        // the window; min/max re-stamp the IBIS models at
                        // their corners and run the SAME window so the
                        // panel overlays a true envelope. A corner whose
                        // tables are absent contributes nothing.
                        ann.transients = {
                            let (mut traces, dur) = run_ibis_transient_traces(
                                converter.take_ibis_drives(), &circuit_ref, &result,
                                "typ", None);
                            if !traces.is_empty() {
                                for (corner, label) in [
                                    (bhdl_spice::ibis::Corner::Min, "min"),
                                    (bhdl_spice::ibis::Corner::Max, "max"),
                                ] {
                                    converter.set_ibis_corner_override(Some(corner));
                                    let Ok(c2) = converter.convert(&netlist) else { continue };
                                    let drives2 = converter.take_ibis_drives();
                                    let Ok((r2, cref2)) =
                                        bhdl_spice::input_draw::solve_dc_with_input_draws(
                                            c2, &regulator_hints(&netlist))
                                    else { continue };
                                    let (t2, _) = run_ibis_transient_traces(
                                        drives2, &cref2, &r2, label, Some(dur));
                                    traces.extend(t2);
                                }
                                converter.set_ibis_corner_override(None);
                            }
                            traces
                        };
                        Some(ann)
                    }
                    Err(e) => {
                        eprintln!("{}", format!("  DC simulation failed: {} (schematic will lack V/I annotations)", e).yellow());
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("{}", format!("  Circuit conversion failed: {} (schematic will lack V/I annotations)", e).yellow());
                None
            }
        }
    };

    // V4 idiom-composed sheet tree (docs/spec/Schematic_V4.md) — rendered
    // from the same netlist AFTER the DC solve, so the sheets carry the
    // solved operating point and the stdlib symbol declarations. This IS
    // the schematic view now: --svg-v4 writes the interactive SVG binding,
    // and the default HTML output is the bound multipage document (the
    // 4.7k-line JS layout engine is retired).
    let v4_sheets = {
        let title = source_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        // Refdes labels: READ the `refdes` attribute the synthesizer's
        // phase-12.7 allocator stamped (single authority, persistent
        // sidecar) — the schematic never mints its own numbering.
        let mut refdes_map = std::collections::HashMap::new();
        for inst in netlist.instances.values() {
            if let Some(rd) = inst.attributes.get("refdes") {
                refdes_map.insert(inst.name.clone(), rd.clone());
            }
        }

        let decor = bhdl_schematic::v4::svg::SheetDecor {
            sim: sim_annotations.as_ref(),
            symbols: Some(&analysis.symbol_definitions),
            refdes: Some(&refdes_map),
        };
        // Hierarchical boards render as a SHEET TREE: the top sheet's
        // entity blocks hyperlink (native SVG <a>) to per-entity sheets
        // written as sibling files "{stem}__{entity}.svg".
        let stem = svg_v4
            .and_then(|p| std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| title.clone());
        let slugify = |s: &str| -> String {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect()
        };
        let href_for = |parent: &str| format!("{stem}__{}.svg", slugify(parent));
        let sheets = bhdl_schematic::v4::render_sheet_tree(&netlist, &title, &decor, &href_for);
        let n_sheets = sheets.len();
        let (mut unidiomized, mut collisions, mut drift) = (0usize, 0usize, 0usize);
        for sheet in &sheets {
            unidiomized += sheet.unidiomized;
            collisions += sheet.collisions;
            drift = drift.max(sheet.label_drift);
        }
        if let Some(svg_path) = svg_v4 {
            let out_path = std::path::Path::new(svg_path);
            for sheet in &sheets {
                if sheet.slug.is_empty() {
                    std::fs::write(out_path, &sheet.svg)?;
                } else {
                    let child = out_path.with_file_name(format!("{stem}__{}.svg", slugify(&sheet.slug)));
                    std::fs::write(&child, &sheet.svg)?;
                    println!("    {} sheet: {}", "→".cyan(), child.display());
                }
            }
            let pages = if n_sheets > 1 { format!(", {n_sheets} sheets") } else { String::new() };
            println!(
                "  {} V4 SVG: {svg_path} ({unidiomized} unidiomized, {collisions} collisions, drift {drift}px{pages})",
                "✓".green(),
            );
        }

        // Bound multipage document: the same sheets, one self-contained
        // HTML (interactive via internal anchors; print to PDF from any
        // browser). Written to --binder when given, and it IS the default
        // HTML output below.
        let href_map: Vec<(String, String)> = sheets
            .iter()
            .filter(|s| !s.slug.is_empty())
            .map(|s| {
                let slug = slugify(&s.slug);
                (format!("{stem}__{slug}.svg"), slug)
            })
            .collect();
        let bound = bhdl_schematic::v4::bind_sheets(
            &title,
            &sheets,
            &href_map,
            env!("CARGO_PKG_VERSION"),
        );
        if let Some(binder_path) = binder {
            std::fs::write(binder_path, &bound)?;
            println!(
                "  {} binder: {binder_path} ({} pages — print to PDF from a browser)",
                "✓".green(),
                n_sheets + 1,
            );
        }
        (bound, n_sheets)
    };

    // Auto-create input filter caps for rails with |> input_filtering in stage chain
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let rail_specs = bhdl_synthesizer::input_cap_sizer::collect_rails_needing_input_filter(
            flow_tracker, &analysis,
        );
        if !rail_specs.is_empty() {
            let auto_names = bhdl_synthesizer::input_cap_sizer::auto_create_input_filter_caps(
                &mut netlist, &rail_specs,
            );
            if !auto_names.is_empty() {
                println!("  {} auto-created input filter caps: {}",
                    "✓".green(), auto_names.join(", "));
            }
        }
    }

    // Size input filter caps using actual GLACIER cascade-corrected currents
    if let Some(ref annotations) = sim_annotations {
        let input_sizing_results = bhdl_synthesizer::input_cap_sizer::size_input_filter_caps(
            &mut netlist, annotations,
        );
        if !input_sizing_results.is_empty() {
            println!("  {} input caps sized for {} rail(s)",
                "✓".green(), input_sizing_results.len());
            for r in &input_sizing_results {
                println!("    {} {}: {:.0}µF bulk (computed from {:.0}mA across {} regulators, ripple target: {:.0}mV)",
                    "→".cyan(), r.cap_name, r.computed_bulk_uf, r.total_load_ma, r.regulator_count, r.ripple_target_mv);
            }
        }
    }

    // Auto-create output filter caps for rails with |> output_filtering in stage chain
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let output_specs = bhdl_synthesizer::output_cap_sizer::collect_rails_needing_output_filter(
            flow_tracker, &analysis,
        );
        if !output_specs.is_empty() {
            let auto_names = bhdl_synthesizer::output_cap_sizer::auto_create_output_filter_caps(
                &mut netlist, &output_specs,
            );
            if !auto_names.is_empty() {
                println!("  {} auto-created output filter caps: {}",
                    "✓".green(), auto_names.join(", "));
            }
        }
    }

    // Size output filter caps using GLACIER data
    if let Some(ref annotations) = sim_annotations {
        let output_sizing = bhdl_synthesizer::output_cap_sizer::size_output_filter_caps(
            &mut netlist, annotations,
        );
        if !output_sizing.is_empty() {
            println!("  {} output caps sized for {} rail(s)",
                "✓".green(), output_sizing.len());
            for r in &output_sizing {
                println!("    {} {}: {:.0}µF (load {:.0}mA, ripple target: {:.0}mV, type: {})",
                    "→".cyan(), r.cap_name, r.computed_cap_uf, r.load_current_ma, r.ripple_target_mv, r.regulator_type);
            }
        }
    }

    // Cap-bank sizers may have minted new instances (C1_2, …) after the
    // synthesizer's phase-12.7 allocation — stamp their refdes now, from
    // the same sidecar (idempotent for everything already stamped).
    bhdl_synthesizer::refdes_alloc::assign_refdes(
        &mut netlist,
        Some(&source_path.with_extension("bhdl.refdes")),
    );

    // Apply GLACIER-driven physical selection (package, voltage rating, etc.)
    if let Some(ref annotations) = sim_annotations {
        let results = bhdl_synthesizer::glacier_physical_selection::apply_glacier_physical_selection(
            &mut netlist,
            &annotations.instance_currents,
            &annotations.instance_power,
            &annotations.net_voltages,
        );
        // Catalog-driven override: where a part_family covers the part's
        // value + derated stress, pick the smallest adequate package from
        // the catalogue (and snap the value) instead of the hardcoded
        // ladder. Uses the same GLACIER-derived stress.
        let families = harvest_catalog_families();
        let overridden = bhdl_synthesizer::glacier_physical_selection::apply_catalog_physical_selection(
            &mut netlist,
            &families,
            &annotations.instance_currents,
            &annotations.instance_power,
            &annotations.net_voltages,
        );
        if overridden > 0 {
            println!(
                "  {} catalog selection: {} part(s) → smallest adequate package",
                "✓".green(),
                overridden
            );
        }
        // visualization uses env-driven supply policy (no CLI flags here)
        let supply_opts =
            bhdl_synthesizer::glacier_physical_selection::SupplyOptions::default()
                .with_env_fallback();
        let mpns = bhdl_synthesizer::glacier_physical_selection::apply_supply_chain_mpns(
            &mut netlist,
            &supply_opts,
            &annotations.net_voltages,
            &annotations.instance_power,
        )
        .len();
        if mpns > 0 {
            println!(
                "  {} supply chain: {} real MPN(s) resolved",
                "✓".green(),
                mpns
            );
        }
        if !results.is_empty() {
            println!("  {} physical parameters selected for {} components",
                "✓".green(), results.len());
        }
    }

    // Extract schematic data from netlist (with sidecar .refdes LUT for stable reference designators)
    let schematic_data = bhdl_schematic::extract_schematic_data(&netlist, Some(&analysis), sim_annotations, Some(source_path))
        .map_err(|e| anyhow::anyhow!("Schematic extraction failed: {}", e))?;

    // Output file generation:
    // --json       → machine-readable SchematicData JSON
    // (default)    → the V4 bound document (multipage HTML; print → PDF)
    if json_output {
        let json = serde_json::to_string_pretty(&schematic_data)?;
        let output_path = output.unwrap_or_else(|| PathBuf::from("circuit.json"));
        fs::write(&output_path, &json)?;
        println!("{}", "✓ Schematic JSON generated".green().bold());
        println!("  Output: {}", output_path.display());
    } else {
        let (bound, n_sheets) = v4_sheets;
        let output_path = output.unwrap_or_else(|| PathBuf::from("circuit.html"));
        fs::write(&output_path, &bound)?;
        println!("{}", "✓ V4 schematic document generated".green().bold());
        println!("  Output: {} ({} sheet{})", output_path.display(), n_sheets, if n_sheets == 1 { "" } else { "s" });
        println!("  Open in a browser (internal links navigate sheets; ⌘P prints to PDF)");
    }

    Ok(())
}

async fn run_layout(
    source_file: &SourceFile,
    output: Option<PathBuf>,
    trials: usize,
    max_iterations: usize,
    html: bool,
    source_path: &Path,
    seed: u64,
) -> Result<()> {
    use bhdl_pnr::semantic::{self, SemanticConfig};
    use bhdl_pnr::types::PnrConfig;

    println!("{}", "PCB Place & Route".bold().cyan());

    // 1. Analyze
    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;
    println!("  {} Analysis complete ({} diagnostics)", "✓".green(), analysis.diagnostics.len());

    // 2. Synthesize
    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(source_path.with_extension("bhdl.refdes"));
    let mut netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;
    println!("  {} Synthesis: {} instances", "✓".green(), netlist.instances.len());

    // 3. Stamp intents
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        bhdl_synthesizer::intent_attribute_stamper::stamp_intent_attributes(&mut netlist, flow_tracker);
    }

    // 4. Expand
    bhdl_synthesizer::expansion_interpreter::expand_entity_instances_with_designs(
        &mut netlist, &analysis.expansion_recipes, &analysis.design_recipes, &analysis.entity_attribute_index,
        &analysis.entity_param_names,
        &analysis.entity_attr_param_refs,
    );
    println!("  {} Expansion: {} instances", "✓".green(), netlist.instances.len());

    // Snap computed passive values to catalog E-series before layout/sim.
    snap_catalog_values(&mut netlist);
    run_value_derivation(&mut netlist, &analysis, source_path)?;

    // 5. GLACIER DC
    let sim_annotations = {
        let mut converter = bhdl_spice::NetlistToSpiceConverter::new();
        match converter.convert(&netlist) {
            Ok(circuit) => {
                let circuit_ref = circuit.clone();
                let solver = bhdl_spice::GlacierDcSolver::new();
                match solver.solve(circuit) {
                    Ok(result) => {
                        println!("  {} GLACIER DC: {} iterations", "✓".green(), result.iterations);
                        Some(build_simulation_annotations(&result, &circuit_ref))
                    }
                    Err(e) => {
                        eprintln!("  {} GLACIER DC failed: {}", "⚠".yellow(), e);
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("  {} Circuit conversion failed: {}", "⚠".yellow(), e);
                None
            }
        }
    };

    // 6. Physical selection
    if let Some(ref ann) = sim_annotations {
        let phys = bhdl_synthesizer::glacier_physical_selection::apply_glacier_physical_selection(
            &mut netlist, &ann.instance_currents, &ann.instance_power, &ann.net_voltages,
        );
        println!("  {} Physical selection: {} components", "✓".green(), phys.len());
    }

    // 7. Build PnR board. The stackup is an INPUT to PnR: a board's
    // `layout Board { layer_stackup N; }` declaration wins; otherwise
    // the synthesizer infers one and REPORTS it (a silent layer-count
    // decision is a hidden design decision).
    let mut sem_config = SemanticConfig::default();
    let board_module_name = netlist
        .top_level_module
        .and_then(|id| netlist.modules.get(id))
        .map(|m| m.name.clone());
    if let Some(name) = &board_module_name {
        if let Some(lay) = analysis.layout_definitions.get(name) {
            if let Some(n) = lay.layer_stackup {
                use bhdl_pnr::types::{StackupPreset, StackupSource};
                let preset = match n {
                    2 => StackupPreset::TwoLayer,
                    4 => StackupPreset::FourLayer,
                    6 => StackupPreset::SixLayer,
                    8 => StackupPreset::EightLayer,
                    other => anyhow::bail!(
                        "layout {name} declares layer_stackup {other}; \
                         supported stackups are 2, 4, 6, 8 layers"
                    ),
                };
                sem_config.board_config.stackup = StackupSource::Preset(preset);
                println!(
                    "  {} Stackup: {n}-layer (declared in layout {name})",
                    "✓".green()
                );
            }
        }
    }
    // Mechanical contract from the layout block: chassis-locked parts,
    // declared outline, mounting holes, keepouts — INPUT that PnR works
    // within (like the stackup), reported with provenance.
    let mut mech_gate: Option<(String, Vec<(f64, f64)>, Vec<(f64, f64, f64)>)> = None;
    if let Some(name) = &board_module_name {
        if let Some(lay) = analysis.layout_definitions.get(name) {
            use bhdl_pnr::types::{
                BoardOutline, BoardSide, FixedPlacement, KeepoutTarget, KeepoutZone,
                MountingHole, ZoneShape,
            };
            for &(ref handle, x, y, rot, back) in &lay.places {
                sem_config.board_config.fixed_placements.push(FixedPlacement {
                    instance_name: handle.clone(),
                    x_mm: x,
                    y_mm: y,
                    rotation_deg: rot,
                    side: if back { BoardSide::Bottom } else { BoardSide::Top },
                    edge: None,
                });
                println!(
                    "  {} Locked: {handle} at ({x}, {y}) rot {rot}°{} (layout {name})",
                    "✓".green(),
                    if back { " side back" } else { "" }
                );
            }
            for &(ref handle, x0, y0, x1, y1) in &lay.region_places {
                use bhdl_pnr::types::{PlacementRegion, ZoneShape};
                sem_config.board_config.placement_regions.push(PlacementRegion {
                    name: format!("region_{handle}"),
                    shape: ZoneShape::Rectangle {
                        x: x0.min(x1),
                        y: y0.min(y1),
                        w: (x1 - x0).abs(),
                        h: (y1 - y0).abs(),
                    },
                    preferred_instances: vec![handle.clone()],
                    weight: 5.0,
                });
                println!(
                    "  {} Region-locked: {handle} within ({x0},{y0})-({x1},{y1}) (layout {name})",
                    "✓".green()
                );
            }
            if let Some((w, h)) = lay.outline_rect {
                sem_config.board_config.outline =
                    BoardOutline::Rectangle { width_mm: w, height_mm: h };
                println!("  {} Outline: rect {w}×{h}mm (declared)", "✓".green());
            } else if let Some(pts) = &lay.outline_polygon {
                sem_config.board_config.outline = BoardOutline::Polygon(pts.clone());
                println!(
                    "  {} Outline: polygon, {} vertices (declared)",
                    "✓".green(),
                    pts.len()
                );
            }
            for &(x, y, drill, keep) in &lay.mounting_holes {
                sem_config.board_config.mounting_holes.push(MountingHole {
                    x_mm: x,
                    y_mm: y,
                    drill_mm: drill,
                    keepout_mm: keep,
                });
            }
            if !lay.mounting_holes.is_empty() {
                println!(
                    "  {} Mounting holes: {} (declared)",
                    "✓".green(),
                    lay.mounting_holes.len()
                );
            }
            for &(x0, y0, x1, y1) in &lay.keepouts {
                sem_config.board_config.keepout_zones.push(KeepoutZone {
                    shape: ZoneShape::Rectangle {
                        x: x0.min(x1),
                        y: y0.min(y1),
                        w: (x1 - x0).abs(),
                        h: (y1 - y0).abs(),
                    },
                    applies_to: KeepoutTarget::All,
                });
            }
            if !lay.keepouts.is_empty() {
                println!(
                    "  {} Keepouts: {} (declared)",
                    "✓".green(),
                    lay.keepouts.len()
                );
            }
            for &(x0, y0, x1, y1) in &lay.cutouts {
                sem_config.board_config.cutouts.push((
                    x0.min(x1),
                    y0.min(y1),
                    x0.max(x1),
                    y0.max(y1),
                ));
                // Components must stay out of the aperture too.
                sem_config.board_config.keepout_zones.push(KeepoutZone {
                    shape: ZoneShape::Rectangle {
                        x: x0.min(x1),
                        y: y0.min(y1),
                        w: (x1 - x0).abs(),
                        h: (y1 - y0).abs(),
                    },
                    applies_to: KeepoutTarget::All,
                });
            }
            if !lay.cutouts.is_empty() {
                println!(
                    "  {} Cutouts: {} (declared)",
                    "✓".green(),
                    lay.cutouts.len()
                );
            }
            if let Some(dxf) = &lay.mech_check {
                let outline_pts: Vec<(f64, f64)> =
                    if let Some(pts) = &lay.outline_polygon {
                        pts.clone()
                    } else if let Some((w, h)) = lay.outline_rect {
                        vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
                    } else {
                        anyhow::bail!(
                            "mech_check requires a declared outline (rect or \
                             polygon) — an auto-sized board has no mechanical \
                             contract to verify"
                        );
                    };
                let holes: Vec<(f64, f64, f64)> = lay
                    .mounting_holes
                    .iter()
                    .map(|&(x, y, d, _)| (x, y, d))
                    .collect();
                println!("  {} Mech check: {dxf} (declared)", "✓".green());
                mech_gate = Some((dxf.clone(), outline_pts, holes));
            }
        }
    }
    let declared_stackup =
        !matches!(sem_config.board_config.stackup, bhdl_pnr::types::StackupSource::Auto);
    let board_result = semantic::build_board(
        &netlist,
        sim_annotations.as_ref(),
        sem_config,
    );
    let mut board = match board_result {
        Ok(b) => b,
        Err(e) if e.to_string().contains("no instances")
            || e.to_string().contains("no top-level module")
            || e.to_string().contains("EmptyNetlist")
            || format!("{e:?}").contains("EmptyNetlist") =>
        {
            // A board with nothing to place is a no-op, not a failure —
            // several corpus fixtures are pure syntax/power tests.
            println!("  {} nothing to lay out (no placeable instances) — skipping", "→".cyan());
            return Ok(());
        }
        Err(e) => return Err(anyhow::anyhow!("Board construction failed: {}", e)),
    };
    // Attach placement recipes from analyzer (vendor datasheet layouts)
    board.placement_recipes = analysis
        .placement_recipes
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !board.placement_recipes.is_empty() {
        println!("  {} Placement recipes: {} entities", "✓".green(), board.placement_recipes.len());
    }
    // Lower expansion/board layout intents into geometric constraints.
    // No-op for un-annotated boards; emits proximity/loop-area constraints
    // for components carrying `for INTENT(...)` annotations.
    let lowering = bhdl_pnr::intent::lower_board_intents(&mut board);
    if lowering.constraints_emitted > 0 {
        println!(
            "  {} Intent constraints: {} from {} annotated components",
            "✓".green(),
            lowering.constraints_emitted,
            lowering.components_with_intent
        );
    }
    if !declared_stackup {
        println!(
            "  {} Stackup: {}-layer (inferred from {} components / {} nets — declare \
             `layout <Board> {{ layer_stackup N; }}` to pin it)",
            "✓".green(),
            board.layer_stack.layers.len(),
            board.components.len(),
            board.nets.len()
        );
    }
    println!("  {} Board: {} components, {} nets, {} groups",
        "✓".green(), board.components.len(), board.nets.len(), board.groups.len());

    // 8. Place & Route (best of N trials)
    println!("  {} Running {} placement trial(s)...", "→".cyan(), trials);
    let config = PnrConfig {
        max_iterations,
        ..PnrConfig::default()
    };
    let result = bhdl_pnr::place_and_route_best_of(board, config, trials, seed)?;

    // 9. Results
    println!("\n{}", "PnR Results".bold().cyan());
    println!("  HPWL:          {:.1} mm", result.metrics.hpwl_mm);
    println!("  Routed length: {:.1} mm", result.metrics.total_routed_length_mm);
    println!("  Via count:     {}", result.metrics.via_count);
    println!("  Routability:   {:.1}%", result.metrics.routability_pct);
    println!("  DRC violations: {}", result.drc_violations.len());

    // 10. Export KiCad PCB
    // Verification report
    let report = bhdl_pnr::output::verify::verify(&result.board, &result.routes);
    bhdl_pnr::output::verify::print_report(&report);

    let pcb = bhdl_pnr::output::kicad::export_kicad_pcb(&result.board, &result.routes);
    // Sibling .kicad_pro: KiCad stores netclass rules in the PROJECT
    // file, and its DRC falls back to defaults (0.2mm clearance) without
    // one — the board must travel with OUR derived rules or the oracle
    // grades against someone else's. kicad-cli picks it up by basename.
    let project_json = format!(
        r#"{{
  "board": {{
    "design_settings": {{
      "rules": {{
        "min_clearance": {sp},
        "min_track_width": {tw}
      }}
    }}
  }},
  "meta": {{ "filename": "board.kicad_pro", "version": 1 }},
  "net_settings": {{
    "classes": [
      {{
        "name": "Default",
        "clearance": {sp},
        "track_width": {tw},
        "via_diameter": 0.6,
        "via_drill": 0.3,
        "bus_width": 12,
        "diff_pair_gap": 0.25,
        "diff_pair_via_gap": 0.25,
        "diff_pair_width": 0.2,
        "line_style": 0,
        "microvia_diameter": 0.3,
        "microvia_drill": 0.1,
        "pcb_color": "rgba(0, 0, 0, 0.000)",
        "schematic_color": "rgba(0, 0, 0, 0.000)",
        "wire_width": 6
      }}
    ],
    "meta": {{ "version": 3 }}
  }}
}}
"#,
        sp = result.board.config.min_spacing_mm,
        tw = result.board.config.min_trace_width_mm,
    );
    let output_path = output.unwrap_or_else(|| {
        let stem = source_path.file_stem().unwrap_or_default().to_string_lossy();
        PathBuf::from(format!("{}.kicad_pcb", stem))
    });
    std::fs::write(&output_path, &pcb)?;
    std::fs::write(output_path.with_extension("kicad_pro"), &project_json)?;
    println!("\n  {} KiCad PCB: {} ({} bytes)", "✓".green(), output_path.display(), pcb.len());

    // MCAD parity gate: the exported board's mechanical contract must
    // match the referenced DXF, or the build fails.
    if let Some((dxf_rel, outline_pts, holes)) = &mech_gate {
        let dxf_path = source_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(dxf_rel);
        let dxf = mech_check::parse_dxf(&dxf_path)?;
        let board_h = outline_pts
            .iter()
            .map(|&(_, y)| y)
            .fold(0.0_f64, f64::max);
        mech_check::check_parity(outline_pts, holes, &dxf, board_h)?;
    }

    // HTML visualization (always generate, or only with --html flag)
    if html {
        let html_content = bhdl_pnr::output::html::export_html(&result.board, &result.routes, &result.metrics);
        let html_path = output_path.with_extension("html");
        std::fs::write(&html_path, &html_content)?;
        println!("  {} HTML board: {} ({} bytes)", "✓".green(), html_path.display(), html_content.len());
    }

    Ok(())
}

async fn run_spice(source_file: &SourceFile, analysis_type: &str, _output: Option<PathBuf>, use_metadata: bool, sku: Option<&str>) -> Result<()> {
    // Run pipeline to get netlist
    let analysis_result = analyze(source_file);
    gate_constructor_args(&analysis_result)?;

    let mut generator = NetlistGenerator::new();
    let mut netlist = generator.generate_from_ast_and_analysis(source_file, &analysis_result).await?;

    // Expand virtual-pin composites (e.g. SignalTubeStage, switching
    // regulators) before SPICE conversion — otherwise a design built from
    // virtual components would be simulated with its expansion missing.
    // Mirrors the synthesize / visualization / pipeline paths.
    if let Some(ref flow_tracker) = analysis_result.flow_tracker {
        bhdl_synthesizer::intent_attribute_stamper::stamp_intent_attributes(&mut netlist, flow_tracker);
    }
    bhdl_synthesizer::expansion_interpreter::expand_entity_instances_with_designs(
        &mut netlist, &analysis_result.expansion_recipes, &analysis_result.design_recipes, &analysis_result.entity_attribute_index,
        &analysis_result.entity_param_names, &analysis_result.entity_attr_param_refs);

    // Apply the selected SKU variant's patches AFTER expansion (so
    // patches address post-expansion instance names). Without this,
    // `bhdl-cli ... --sku Pro bom` would size R_FB at the variant's
    // override but `bhdl-cli ... --sku Pro spice` would simulate
    // with the base value — internally inconsistent.
    apply_sku_variant(&analysis_result, &mut netlist, sku)?;

    // Stage 3: snap computed passive values to catalog E-series so SPICE
    // simulates the as-built (orderable) value, not the raw computed one.
    snap_catalog_values(&mut netlist);

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
    gate_constructor_args(&analysis)?;
    
    // TODO: AnalysisResult doesn't implement Serialize
    // let analysis_path = output_dir.join("analysis.json");
    // fs::write(&analysis_path, serde_json::to_string_pretty(&analysis)?)?;
    // println!("  ✓ Analysis saved to {}", analysis_path.display());
    println!("  ✓ Analysis complete (JSON export not implemented)");
    
    // Step 2: Synthesis
    println!("\n{}", "2. Synthesis".blue().bold());
    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(_input_path.with_extension("bhdl.refdes"));
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;
    
    let netlist_path = output_dir.join("netlist.json");
    fs::write(&netlist_path, serde_json::to_string_pretty(&netlist)?)?;
    println!("  ✓ Netlist saved to {}", netlist_path.display());
    
    // Step 3: Visualization
    if !no_viz {
        println!("\n{}", "3. Visualization".blue().bold());

        // V4 bound document (the JS canvas viewer is retired). Labels READ
        // the `refdes` attribute the synthesizer's allocator stamped —
        // never a second allocator here, or the schematic's R1 and the
        // BOM's R1 could name different parts. Minting is a fallback for
        // unstamped instances only, and walks name-sorted so even the
        // fallback numbers deterministically.
        let mut lut = bhdl_schematic::RefDesLut::default();
        let mut refdes_map = std::collections::HashMap::new();
        let mut insts: Vec<_> = netlist.instances.values().collect();
        insts.sort_by(|a, b| a.name.cmp(&b.name));
        for inst in insts {
            let is_phantom = netlist
                .modules
                .get(inst.definition)
                .map(|m| m.name == inst.name)
                .unwrap_or(false);
            if is_phantom {
                continue;
            }
            if let Some(rd) = inst.attributes.get("refdes") {
                refdes_map.insert(inst.name.clone(), rd.clone());
                continue;
            }
            let class = inst
                .attributes
                .get("component_class")
                .map(String::as_str)
                .unwrap_or("");
            let category = match class {
                "voltage_regulator" | "ldo" | "switching_regulator" => "regulator",
                "" => "ic",
                other => other,
            };
            let prefix = bhdl_schematic::category_to_prefix(category);
            let prefix = if prefix == "X" { "U" } else { prefix };
            refdes_map.insert(inst.name.clone(), lut.assign(prefix, &inst.name));
        }
        let decor = bhdl_schematic::v4::svg::SheetDecor {
            sim: None,
            symbols: Some(&analysis.symbol_definitions),
            refdes: Some(&refdes_map),
        };
        let board = output_dir
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "board".into());
        let href_for = |parent: &str| format!("#sheet-{parent}");
        let sheets = bhdl_schematic::v4::render_sheet_tree(&netlist, &board, &decor, &href_for);
        let html = bhdl_schematic::v4::bind_sheets(&board, &sheets, &[], env!("CARGO_PKG_VERSION"));
        let html_path = output_dir.join("circuit.html");
        fs::write(&html_path, html)?;
        println!("  ✓ V4 schematic document saved to {}", html_path.display());

        // Also save SchematicData JSON for tooling
        let schematic_data = bhdl_schematic::extract_schematic_data(&netlist, Some(&analysis), None, None)
            .map_err(|e| anyhow::anyhow!("Schematic extraction failed: {}", e))?;
        let json_path = output_dir.join("schematic.json");
        fs::write(&json_path, serde_json::to_string_pretty(&schematic_data)?)?;
        println!("  ✓ Schematic JSON saved to {}", json_path.display());
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
    gate_constructor_args(&analysis)?;
    
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

/// Generate a manufacturing BOM by walking the post-expansion
/// netlist. Mirrors the synthesis prologue used by run_spice /
/// run_visualization — analyze, stamp intent attributes, run the
/// expansion interpreter — so the BOM sees every concrete leaf
/// component (including vendor-design-recipe-sized passives and
/// expansion-block children for entity families like SignalTubeStage
/// and BjtCurrentMirror).
/// Apply the user-selected SKU variant's patches to the netlist.
/// Errors out (with the list of declared variants) when the board
/// has variants and the user didn't pass `--sku`. Returns
/// successfully without touching the netlist when the board has no
/// variants — the implicit "default" SKU.
/// E-series value snapping — sizing-pipeline stage 3 (catalog-authoritative).
///
/// Rewrites each passive instance's `value` to the nearest standard value
/// of the E-series its matching `part_family` catalog entry declares, so
/// the value SPICE simulates and the value the BOM names are one real,
/// orderable number (a design-block-computed 31250Ω → 31.6kΩ). Catalog
/// families are discovered through the library system — the same resolver
/// that resolves imports — so the E-series is never hardcoded per type.
///
/// Best-effort and side-effect-free when nothing applies: no resolver, no
/// catalogs, or no matching family ⇒ the netlist is left untouched. Called
/// post-expansion / post-SKU-patch, before SPICE conversion and BOM walk
/// (both read the `value` attribute).
/// Discover + parse + harvest the catalog `part_family` declarations
/// (E-series ranges + ratings + package) through the library system, with
/// a bundled-stdlib fallback so it works on any board (no bhdl.toml
/// needed). Shared by the value-only snap and the rating-aware physical
/// selection.
fn harvest_catalog_families() -> Vec<bhdl_analyzer::value_snap::FamilyDecl> {
    use bhdl_ast::AstNode;
    // Prefer the user's import resolver (built only when they opt into a
    // bhdl.toml / -I / $BHDL_LIB_PATH). Otherwise fall back to a
    // discovery-only resolver rooted at the bundled stdlib, so catalog
    // selection works on ANY board — not just ones with a project
    // manifest. (Selection must not be gated behind the manifest opt-in,
    // which is for *import* resolution.)
    let Some(resolver) =
        bhdl_synthesizer::global_library_resolver().or_else(catalog_discovery_resolver)
    else {
        return Vec::new();
    };
    let mut sources = Vec::new();
    for path in resolver.catalog_bhdl_files() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let pr = bhdl_parser::parse(&text);
            if let Some(sf) = SourceFile::cast(pr.syntax()) {
                sources.push(sf);
            }
        }
    }
    bhdl_analyzer::value_snap::harvest_families(&sources)
}

fn snap_catalog_values(netlist: &mut bhdl_netlist::Netlist) {
    let families = harvest_catalog_families();
    if families.is_empty() {
        return;
    }
    let n = bhdl_analyzer::value_snap::snap_netlist_values(netlist, &families);
    if n > 0 {
        println!(
            "  {} E-series snap: {} value(s) → standard parts (catalog-driven)",
            "✓".green(),
            n
        );
    }
}

/// Fallback resolver used only for *catalog discovery* (E-series value
/// snapping) when the user hasn't opted into a project manifest. Rooted at
/// the bundled stdlib, located the same way the legacy import path locates
/// it — `bhdl-stdlib` relative to the working directory. Returns `None`
/// if that directory isn't present (e.g. an installed CLI run from an
/// unrelated cwd), in which case snapping is simply skipped. Threads no
/// manifest, so its `library_roots()` is exactly the stdlib — it never
/// affects import resolution.
fn catalog_discovery_resolver() -> Option<bhdl_common::library::LibraryResolver> {
    let stdlib = bhdl_common::import_search::locate_dir("bhdl-stdlib")?;
    bhdl_common::library::LibraryResolver::new(None, &[], None, Some(stdlib)).ok()
}

fn apply_sku_variant(
    analysis: &bhdl_analyzer::AnalysisResult,
    netlist: &mut bhdl_netlist::Netlist,
    selected: Option<&str>,
) -> Result<()> {
    // Aggregate every board's declared variants into one map so the
    // CLI doesn't need to know which board name was synthesized.
    // (v0.1 boards-with-variants live in a single .bhdl file, so the
    // map shape simplifies to "name → Variant".)
    let mut all_variants: std::collections::HashMap<String, &bhdl_common::variant::Variant>
        = std::collections::HashMap::new();
    for (_board, by_name) in &analysis.variants {
        for (name, variant) in by_name {
            all_variants.insert(name.clone(), variant);
        }
    }

    if all_variants.is_empty() {
        // No variants declared anywhere on the board. The implicit
        // "default" SKU is the base design — silently proceed.
        if let Some(name) = selected {
            if name != "default" {
                anyhow::bail!(
                    "--sku '{name}' was requested but the board declares no \
                     variants (it has an implicit single 'default' SKU). \
                     Either remove --sku or declare `variant {name} {{ ... }}` \
                     on the board.");
            }
        }
        return Ok(());
    }

    let name = match selected {
        Some(n) => n,
        None => {
            let mut names: Vec<&String> = all_variants.keys().collect();
            names.sort();
            let listing = names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            anyhow::bail!(
                "This board declares SKU variants; pass --sku <Name> to select one. \
                 Available variants: {listing}");
        }
    };

    let variant = all_variants.get(name).ok_or_else(|| {
        let mut names: Vec<&String> = all_variants.keys().collect();
        names.sort();
        let listing = names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        anyhow::anyhow!("Unknown SKU '{name}'. Available variants: {listing}")
    })?;

    let report = bhdl_synthesizer::variant_apply::apply_variant(netlist, variant);
    println!("  {} variant '{}' applied: {} value override(s), {} DNP, {} missing",
        "✓".green(),
        report.variant_name,
        report.values_changed,
        report.instances_dnpd,
        report.missing_instances.len());
    if !report.missing_instances.is_empty() {
        eprintln!("    {} variant patches referenced unknown instance(s): {}",
            "!".yellow(),
            report.missing_instances.join(", "));
    }
    Ok(())
}

async fn cmd_list_skus(source_file: &SourceFile) -> Result<()> {
    let analysis = analyze(source_file);
    let mut all_names: Vec<String> = analysis.variants.values()
        .flat_map(|by_name| by_name.keys().cloned())
        .collect();
    all_names.sort();
    all_names.dedup();

    if all_names.is_empty() {
        println!("{}", "default".dimmed());
        println!("  (no `variant` blocks declared on this board — single implicit SKU)");
    } else {
        for n in &all_names {
            println!("{n}");
        }
    }
    Ok(())
}

/// Run the board's scheduled IBIS edges (`ibis_wave_<PIN>`) through the
/// time-domain solver and return decimated traces of each driven net for
/// the schematic's scope panel. The solved DC operating point seeds the
/// initial conditions. Duration extends (up to 8× the edge horizon) until
/// the trace has settled, so slow load RCs aren't cut mid-flight.
/// Best-effort: a failed solve returns no traces — an absent panel, never
/// a fabricated one.
fn run_ibis_transient_traces(
    drives: Vec<bhdl_spice::ibis_transient::IbisDrive>,
    circuit: &bhdl_spice::circuit::Circuit,
    dc_result: &bhdl_spice::DcAnalysisResult,
    corner: &str,
    fixed_duration: Option<f64>,
) -> (Vec<bhdl_schematic::TransientTrace>, f64) {
    if drives.is_empty() {
        return (Vec::new(), 0.0);
    }
    let ic: std::collections::HashMap<String, f64> = circuit
        .nodes()
        .map(|(idx, n)| {
            let v = if n.is_ground {
                0.0
            } else {
                dc_result.node_voltages.get(&idx).copied().unwrap_or(0.0)
            };
            (n.name.clone(), v)
        })
        .collect();

    // One probe per driven net, keeping the drive's schedule for the label.
    let mut probes: Vec<(String, String)> = Vec::new(); // (net, spec)
    for d in &drives {
        let net = circuit
            .branches()
            .find(|(_, b)| b.name == d.branch)
            .and_then(|(_, b)| {
                let idx = b.nodes[0];
                circuit.nodes().find(|(i, _)| *i == idx).map(|(_, n)| n.name.clone())
            });
        let Some(net) = net else { continue };
        if probes.iter().any(|(n, _)| *n == net) {
            continue;
        }
        let spec = d
            .events
            .iter()
            .map(|e| {
                format!(
                    "{}@{}n",
                    if e.rising { "rise" } else { "fall" },
                    (e.t * 1e9 * 1000.0).round() / 1000.0
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        probes.push((net, spec));
    }
    if probes.is_empty() {
        return (Vec::new(), 0.0);
    }

    let horizon = drives.iter().map(|d| d.horizon()).fold(0.0, f64::max);
    if horizon <= 0.0 {
        return (Vec::new(), 0.0);
    }
    let mut duration = fixed_duration.unwrap_or(horizon * 1.5);
    let attempts = if fixed_duration.is_some() { 1 } else { 4 };
    let mut result = None;
    for _ in 0..attempts {
        let params = bhdl_spice::transient::TransientParams::new(
            "",
            bhdl_spice::transient::Stimulus::Constant(0.0),
            probes.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
            duration,
            // Coarse-ish steps on purpose: h sets the capacitor companion
            // conductance (C/h) — very small h turns every decoupling cap
            // into a sub-mΩ short and Newton diverges on real boards. 400
            // steps resolves any edge the panel can display.
            duration / 400.0,
        );
        match bhdl_spice::ibis_transient::run_transient_ibis_ic(circuit, &params, &drives, Some(&ic)) {
            Ok(r) => {
                // Settled when every probe moved <1% of its own range over
                // the final 10% of the run.
                let settled = probes.iter().all(|(n, _)| {
                    let tr = &r.probe_voltages[n];
                    let range = tr.iter().cloned().fold(f64::MIN, f64::max)
                        - tr.iter().cloned().fold(f64::MAX, f64::min);
                    let tail = tr.len().saturating_sub(tr.len() / 10);
                    (tr[tail.min(tr.len() - 1)] - tr[tr.len() - 1]).abs() <= 0.01 * range.max(1e-9)
                });
                let done = settled;
                result = Some(r);
                if done {
                    break;
                }
                duration *= 2.0;
            }
            Err(e) => {
                info!("ibis transient trace ({corner}) failed: {e} — no scope panel");
                return (Vec::new(), 0.0);
            }
        }
    }
    let Some(r) = result else { return (Vec::new(), 0.0) };

    // Decimate to ≤160 samples per trace for the SVG.
    let stride = (r.times.len() / 160).max(1);
    let traces: Vec<bhdl_schematic::TransientTrace> = probes
        .into_iter()
        .map(|(net, spec)| {
            let tr = &r.probe_voltages[&net];
            let mut times = Vec::new();
            let mut volts = Vec::new();
            for i in (0..r.times.len()).step_by(stride) {
                times.push(r.times[i]);
                volts.push(tr[i]);
            }
            if *times.last().unwrap() != *r.times.last().unwrap() {
                times.push(*r.times.last().unwrap());
                volts.push(*tr.last().unwrap());
            }
            bhdl_schematic::TransientTrace {
                net,
                spec,
                corner: corner.to_string(),
                times,
                volts,
            }
        })
        .collect();
    (traces, duration)
}

/// Run simulation-driven value derivation (`derive_rule` markers) and
/// print the report. Hard error when a requested derivation can't run.
fn run_value_derivation(
    netlist: &mut bhdl_netlist::netlist::Netlist,
    analysis: &bhdl_analyzer::AnalysisResult,
    source_path: &Path,
) -> Result<()> {
    let rows = value_deriver::derive_values(netlist, analysis, source_path)?;
    if rows.is_empty() {
        return Ok(());
    }
    println!("  {} {} value(s) derived by simulation:", "✓".green(), rows.len());
    for r in &rows {
        println!(
            "    {} {} [{}]: {} → {}  ({})",
            "→".cyan(), r.instance, r.rule, r.seed, r.derived, r.detail
        );
    }
    Ok(())
}

/// `bhdl vendor status|install` — the vendor-file distribution story
/// (bhdl_common::vendor). Vendor simulation data is licensed
/// use-not-redistribute, so the repo commits a MANIFEST (path + sha256 +
/// provenance) and the user obtains the files; `install` verifies a
/// download against the manifest and places it in the local store.
fn cmd_vendor(args: &[String]) -> Result<()> {
    use bhdl_common::vendor::{
        resolve, sha256_file, store_relpath, store_root, VendorManifest,
    };

    let usage = "usage: bhdl vendor status | bhdl vendor install <file-or-dir>...";
    let action = args.first().map(String::as_str).unwrap_or("");

    let cwd = std::env::current_dir()?;
    let manifest_path = VendorManifest::find_upwards(&cwd)
        .ok_or_else(|| anyhow::anyhow!(
            "no vendor/MANIFEST.toml found walking up from {} — run inside a BHDL              project tree", cwd.display()))?;
    let manifest = VendorManifest::load(&manifest_path).map_err(|e| anyhow::anyhow!(e))?;
    let repo_root = manifest_path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());

    match action {
        "status" => {
            println!("{}", "Vendor files (manifest: see vendor/MANIFEST.toml)".bold());
            let mut missing = 0;
            for f in &manifest.files {
                match resolve(&f.path, repo_root.as_deref()) {
                    Some(found) => {
                        let hash = sha256_file(&found)?;
                        if hash == f.sha256 {
                            println!("  {} {}  ({})", "✓".green(), f.path, found.display());
                        } else {
                            println!(
                                "  {} {}  ({}) — sha256 MISMATCH: file differs from the                                  one the stdlib was validated against",
                                "✗".red(), f.path, found.display()
                            );
                        }
                    }
                    None => {
                        missing += 1;
                        println!("  {} {}  MISSING", "✗".red(), f.path);
                        println!("      obtain: {}", f.source);
                        println!("      license: {}", f.license);
                    }
                }
            }
            if missing > 0 {
                println!();
                println!(
                    "  install downloaded files with: bhdl vendor install <file-or-dir>"
                );
            }
            Ok(())
        }
        "install" => {
            let sources = &args[1..];
            if sources.is_empty() {
                anyhow::bail!("{usage}");
            }
            // Collect candidate files from the given files/dirs (recursive).
            let mut candidates: Vec<PathBuf> = Vec::new();
            fn walk(p: &Path, out: &mut Vec<PathBuf>) {
                if p.is_file() {
                    out.push(p.to_path_buf());
                } else if p.is_dir() {
                    if let Ok(rd) = std::fs::read_dir(p) {
                        for e in rd.flatten() {
                            walk(&e.path(), out);
                        }
                    }
                }
            }
            for s in sources {
                walk(Path::new(s), &mut candidates);
            }
            let store = store_root()
                .ok_or_else(|| anyhow::anyhow!("cannot determine vendor store (no HOME, no BHDL_VENDOR_DIR)"))?;

            let mut installed = 0;
            for f in &manifest.files {
                let base = Path::new(&f.path)
                    .file_name()
                    .map(|b| b.to_string_lossy().to_string())
                    .unwrap_or_default();
                let Some(src) = candidates.iter().find(|c| {
                    c.file_name().map(|b| b.to_string_lossy().eq_ignore_ascii_case(&base))
                        .unwrap_or(false)
                }) else {
                    continue;
                };
                let hash = sha256_file(src)?;
                if hash != f.sha256 {
                    println!(
                        "  {} {} — sha256 mismatch, NOT installed (expected the exact                          file the stdlib was validated against; got {hash})",
                        "✗".red(), src.display()
                    );
                    continue;
                }
                let dest = store.join(store_relpath(&f.path));
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(src, &dest)?;
                println!("  {} {} → {}", "✓".green(), f.path, dest.display());
                installed += 1;
            }
            if installed == 0 {
                anyhow::bail!(
                    "nothing installed: no candidate matched a manifest entry by name                      (and hash) — see `bhdl vendor status` for what is expected"
                );
            }
            println!();
            println!("  {} {installed} file(s) in store {}", "✓".green(), store.display());
            Ok(())
        }
        _ => anyhow::bail!("{usage}"),
    }
}

/// `transient` — solve the board through time while its IBIS buffers
/// switch per their `ibis_wave_<PIN>` directives. The board's own rail
/// sources power the circuit; the edges are the stimulus.
async fn cmd_transient(
    source_file: &SourceFile,
    source_path: &Path,
    output: Option<PathBuf>,
    extra_probes: Vec<String>,
    duration: Option<String>,
    timestep: Option<String>,
    corners: bool,
) -> Result<()> {
    println!("{}", "Transient IBIS simulation...".bold());

    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;
    if !analysis.diagnostics.is_empty() {
        eprintln!("{}", "Warning: Analysis found issues".yellow().bold());
    }

    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(source_path.with_extension("bhdl.refdes"));
    let mut netlist = generator
        .generate_from_ast_and_analysis(source_file, &analysis)
        .await
        .context("Failed to synthesize netlist")?;
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        bhdl_synthesizer::intent_attribute_stamper::stamp_intent_attributes(&mut netlist, flow_tracker);
    }
    bhdl_synthesizer::expansion_interpreter::expand_entity_instances_with_designs(
        &mut netlist,
        &analysis.expansion_recipes,
        &analysis.design_recipes,
        &analysis.entity_attribute_index,
        &analysis.entity_param_names,
        &analysis.entity_attr_param_refs,
    );
    snap_catalog_values(&mut netlist);
    run_value_derivation(&mut netlist, &analysis, source_path)?;

    let mut converter = NetlistToSpiceConverter::new();
    converter.set_model_overrides(bhdl_synthesizer::model_evaluator::evaluate_model_overrides(
        &netlist,
        &analysis.model_recipes,
        &analysis.entity_attribute_index,
    ));
    converter.set_ibis_models(
        analysis.model_recipes.iter()
            .filter_map(|(e, r)| (!r.ibis.is_empty()).then(|| (e.clone(), r.ibis.clone())))
            .collect(),
        source_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    );
    // Corner plan: the stanza-declared corner alone, or the full sweep.
    let corner_plan: Vec<(Option<bhdl_spice::ibis::Corner>, &str)> = if corners {
        vec![
            (Some(bhdl_spice::ibis::Corner::Typ), "typ"),
            (Some(bhdl_spice::ibis::Corner::Min), "min"),
            (Some(bhdl_spice::ibis::Corner::Max), "max"),
        ]
    } else {
        vec![(None, "declared")]
    };

    converter.set_ibis_corner_override(corner_plan[0].0);
    let circuit = converter.convert(&netlist).context("Circuit conversion failed")?;
    let drives = converter.take_ibis_drives();
    if drives.is_empty() {
        anyhow::bail!(
            "no scheduled edges: no instance carries an ibis_wave_<PIN> directive              (e.g. u1: Part(ibis_wave_OUT=\"rise@2n\"))"
        );
    }

    // Probes: every driven pin's net, plus whatever the user asked for.
    let mut probes: Vec<String> = Vec::new();
    for d in &drives {
        let net = circuit
            .branches()
            .find(|(_, b)| b.name == d.branch)
            .and_then(|(_, b)| {
                let idx = b.nodes[0];
                circuit.nodes().find(|(i, _)| *i == idx).map(|(_, n)| n.name.clone())
            });
        if let Some(n) = net {
            if !probes.contains(&n) {
                probes.push(n);
            }
        }
    }
    let node_names: std::collections::HashSet<String> =
        circuit.nodes().map(|(_, n)| n.name.clone()).collect();
    for p in extra_probes {
        if !node_names.contains(&p) {
            anyhow::bail!("probe net '{p}' is not in the circuit");
        }
        if !probes.contains(&p) {
            probes.push(p);
        }
    }

    let parse_t = |s: &Option<String>, what: &str| -> Result<Option<f64>> {
        match s {
            None => Ok(None),
            Some(v) => bhdl_spice::ibis_transient::parse_time(v)
                .filter(|t| *t > 0.0)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("bad {what} '{v}' (e.g. \"50n\", \"0.5u\")")),
        }
    };
    let horizon = drives.iter().map(|d| d.horizon()).fold(0.0, f64::max);
    let duration = parse_t(&duration, "duration")?.unwrap_or(horizon * 1.5);
    // Default h = duration/400: h sets the cap-companion conductance
    // (C/h); much smaller steps turn rail decoupling into sub-mΩ shorts
    // and the per-step Newton diverges on real boards.
    let timestep = parse_t(&timestep, "timestep")?.unwrap_or(duration / 400.0);
    println!(
        "  {} {} drive(s), horizon {:.3}ns — simulating {:.3}ns at {:.4}ns steps",
        "✓".green(), drives.len(), horizon * 1e9, duration * 1e9, timestep * 1e9
    );

    // One (label, result) per corner that actually has data. Each corner
    // re-stamps the IBIS models at ITS tables AND re-solves ITS DC
    // operating point before any edge fires: the corner shifts the
    // starting point too, not just the edge.
    let mut runs: Vec<(&str, bhdl_spice::transient::TransientResult)> = Vec::new();
    for (i, (ov, label)) in corner_plan.iter().enumerate() {
        let (drives_c, circuit_c) = if i == 0 {
            (drives.clone(), circuit.clone())
        } else {
            converter.set_ibis_corner_override(*ov);
            let c = converter.convert(&netlist).context("Circuit conversion failed")?;
            (converter.take_ibis_drives(), c)
        };
        if drives_c.is_empty() {
            println!("  {} corner {label}: no table data — skipped", "→".cyan());
            continue;
        }
        // A corner that can't solve is reported and skipped, not fatal —
        // the sweep's job is to show which corners stand and which don't.
        let (dc_result, circuit_c) = match bhdl_spice::input_draw::solve_dc_with_input_draws(
            circuit_c,
            &regulator_hints(&netlist),
        ) {
            Ok(x) => x,
            Err(e) => {
                eprintln!(
                    "  {}",
                    format!("corner {label}: DC operating point did not converge ({e}) — skipped")
                        .yellow()
                );
                continue;
            }
        };
        let ic: std::collections::HashMap<String, f64> = circuit_c
            .nodes()
            .map(|(idx, n)| {
                let v = if n.is_ground {
                    0.0
                } else {
                    dc_result.node_voltages.get(&idx).copied().unwrap_or(0.0)
                };
                (n.name.clone(), v)
            })
            .collect();
        println!(
            "  {} corner {label}: DC operating point solved ({} iterations)",
            "✓".green(), dc_result.iterations
        );
        let params = bhdl_spice::transient::TransientParams::new(
            "", // no external stimulus: rails + edges drive the board
            bhdl_spice::transient::Stimulus::Constant(0.0),
            probes.clone(),
            duration,
            timestep,
        );
        let result = match bhdl_spice::ibis_transient::run_transient_ibis_ic(
            &circuit_c, &params, &drives_c, Some(&ic),
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "  {}",
                    format!("corner {label}: transient did not converge ({e}) — skipped").yellow()
                );
                continue;
            }
        };
        runs.push((label, result));
    }
    if runs.is_empty() {
        anyhow::bail!("no corner had usable IBIS table data");
    }

    // Ground-bounce estimate per drive: peak |L·di/dt| of the SOLVED
    // pin current through the package lead inductance the vendor file
    // declares ([Package] lump or per-pin override). No inductance in
    // the file ⇒ no estimate, visibly.
    println!();
    for d in &drives {
        for (label, result) in &runs {
            let Some(trace) = result.branch_currents.get(&d.branch) else { continue };
            if trace.len() < 3 {
                continue;
            }
            match d.pkg_l {
                Some(l) => {
                    let mut peak = 0.0f64;
                    let mut di_peak = 0.0f64;
                    for w in trace.windows(2).zip(result.times.windows(2)) {
                        let (iw, tw) = w;
                        let dt = tw[1] - tw[0];
                        if dt <= 0.0 {
                            continue;
                        }
                        let didt = (iw[1] - iw[0]) / dt;
                        if didt.abs() > di_peak {
                            di_peak = didt.abs();
                        }
                        peak = peak.max((l * didt).abs());
                    }
                    println!(
                        "  {} {} [{label}]: ground-bounce estimate peak {:.1}mV (pkg L {:.2}nH × di/dt peak {:.2}mA/ns)",
                        "→".cyan(), d.branch, peak * 1e3, l * 1e9, di_peak * 1e-6
                    );
                }
                None => {
                    println!(
                        "  {} {} [{label}]: ground-bounce UNCHECKED — the .ibs file carries no [Package]/per-pin inductance",
                        "→".cyan(), d.branch
                    );
                }
            }
        }
    }

    println!();
    println!(
        "  {:<24} {:<9} {:>10} {:>10} {:>10} {:>10}",
        "net", "corner", "v(0)", "v(end)", "min", "max"
    );
    for p in &probes {
        for (label, result) in &runs {
            let tr = &result.probe_voltages[p];
            let min = tr.iter().cloned().fold(f64::MAX, f64::min);
            let max = tr.iter().cloned().fold(f64::MIN, f64::max);
            println!(
                "  {:<24} {:<9} {:>9.4}V {:>9.4}V {:>9.4}V {:>9.4}V",
                p, label, tr.first().unwrap_or(&0.0), tr.last().unwrap_or(&0.0), min, max
            );
        }
    }

    if let Some(path) = output {
        // Same window and step across corners ⇒ one shared time column;
        // one voltage column per net per corner.
        let times = &runs[0].1.times;
        let mut csv = String::from("time_s");
        for p in &probes {
            for (label, _) in &runs {
                csv.push_str(&format!(",{p}@{label}"));
            }
        }
        csv.push('\n');
        for (i, t) in times.iter().enumerate() {
            csv.push_str(&format!("{t:.6e}"));
            for p in &probes {
                for (_, result) in &runs {
                    csv.push_str(&format!(",{:.6e}", result.probe_voltages[p][i]));
                }
            }
            csv.push('\n');
        }
        fs::write(&path, csv)?;
        println!();
        println!(
            "  {} trace written to {} ({} samples × {} corner(s))",
            "✓".green(), path.display(), times.len(), runs.len()
        );
    }
    Ok(())
}

async fn cmd_bom(
    source_file: &SourceFile,
    source_path: &Path,
    output: Option<PathBuf>,
    format: &str,
    sku: Option<&str>,
    supply_profile: Option<String>,
    supply_qty: Option<u64>,
    supply_net: Vec<String>,
    update_lock: bool,
    locked: bool,
    simulate: bool,
) -> Result<()> {
    use bhdl_analyzer::sku_bom;

    println!("{}", "Generating manufacturing BOM...".bold());

    // 1. Analysis pass.
    let analysis = analyze(source_file);
    gate_constructor_args(&analysis)?;
    if !analysis.diagnostics.is_empty() {
        eprintln!("{}", "Warning: Analysis found issues".yellow().bold());
    }

    // 2. Build the netlist (logical instances).
    let mut generator = NetlistGenerator::new();
    generator.set_refdes_lut_path(source_path.with_extension("bhdl.refdes"));
    let mut netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await
        .context("Failed to synthesize netlist")?;

    // 3. Stamp intent attributes so design recipes see them.
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        bhdl_synthesizer::intent_attribute_stamper::stamp_intent_attributes(&mut netlist, flow_tracker);
    }

    // 4. Expand entity instances — turns SignalTubeStage / BjtCurrentMirror
    //    / etc. into concrete leaf passives. The BOM walker needs to see
    //    the post-expansion instances, not the pre-expansion logical ones.
    let recipe_results = bhdl_synthesizer::expansion_interpreter::expand_entity_instances_with_designs(
        &mut netlist,
        &analysis.expansion_recipes,
        &analysis.design_recipes, &analysis.entity_attribute_index,
        &analysis.entity_param_names,
        &analysis.entity_attr_param_refs,
    );
    if !recipe_results.is_empty() {
        println!("  {} expansion blocks applied for {} entity instance(s)",
            "✓".green(), recipe_results.len());
    }

    // 4.5. Apply the selected SKU variant's patches. If the board
    //      declares variants but the user didn't pass --sku, error
    //      out with a list (same anti-silent-fallback principle as
    //      Stage 6 device discovery).
    apply_sku_variant(&analysis, &mut netlist, sku)?;

    // 4.52. Simulation-driven value derivation (`derive_rule` markers) —
    //       before any solve or catalog snap so the BOM, sign-off and
    //       stress all see the DERIVED value.
    run_value_derivation(&mut netlist, &analysis, &source_path.to_path_buf())?;

    // 4.55. Optional GLACIER DC solve (`--simulate`): derive each passive's
    //       stress from the simulated operating point. This stamps the
    //       inductor's `current_rating` (the L analogue of cap voltage /
    //       resistor power) from the actual branch current, which the
    //       supply-chain current gate then consumes — refining the recipe's
    //       closed-form `rated_current` seed. Best-effort: a failed solve
    //       falls through to the declared-stress path below.
    let sim_stress: Option<bhdl_schematic::SimulationAnnotations> = if simulate {
        let mut converter = NetlistToSpiceConverter::new();
        match converter.convert(&netlist) {
            Ok(circuit) => {
                let circuit_ref = circuit.clone();
                match bhdl_spice::GlacierDcSolver::new().solve(circuit) {
                    Ok(result) => {
                        println!(
                            "  {} GLACIER DC solve: converged in {} iteration(s)",
                            "✓".green(),
                            result.iterations
                        );
                        Some(build_simulation_annotations(&result, &circuit_ref))
                    }
                    Err(e) => {
                        eprintln!("  {}", format!("DC solve failed ({e}); using declared stress").yellow());
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("  {}", format!("circuit conversion failed ({e}); using declared stress").yellow());
                None
            }
        }
    } else {
        None
    };
    if let Some(ref ann) = sim_stress {
        // Stamp the inductor current gate from the simulated operating point
        // (current only — package/value selection stays with the catalogue
        // pass below, identical to the non-sim path, to avoid disturbing the
        // footprint/value).
        let n = bhdl_synthesizer::glacier_physical_selection::stamp_inductor_sim_current(
            &mut netlist,
            &ann.instance_currents,
        );
        if n > 0 {
            println!(
                "  {} sim stress: {} inductor current rating(s) from GLACIER operating point",
                "✓".green(),
                n
            );
        }
    }

    // 4.6. Catalog selection for the BOM: snap the value AND assign the
    //      smallest adequate package using the DECLARED rail voltages for cap
    //      voltage stress (analytic, always available). `--simulate` adds the
    //      inductor current gate above; package/value selection here is
    //      identical with or without it, so the BOM always names a real,
    //      orderable part.
    {
        let families = harvest_catalog_families();
        let declared_v =
            bhdl_synthesizer::glacier_physical_selection::declared_net_voltages(&netlist);
        let empty = std::collections::HashMap::new();
        let n = bhdl_synthesizer::glacier_physical_selection::apply_catalog_physical_selection(
            &mut netlist,
            &families,
            &empty,
            &empty,
            &declared_v,
        );
        if n > 0 {
            println!(
                "  {} catalog selection: {} part(s) → standard value + smallest adequate package",
                "✓".green(),
                n
            );
        }

        // 4.65. Apply Stage-C sign-off value-stepping to the BOM (`--simulate`).
        //       The inductor ripple-ratio step is analytic (operating point from
        //       the rails + regulator attributes, no GLACIER solve), so it runs
        //       here, after the snap and BEFORE MPN resolution: mutate the value,
        //       re-package at the new value, then the supply gate below resolves
        //       the stepped part's real MPN. This closes the loop from
        //       "recommend 6.8µH" to a BOM that actually carries it.
        if simulate {
            let applied = bhdl_synthesizer::signoff::apply_inductor_stepping(
                &mut netlist,
                &analysis.entity_attribute_index,
            );
            if !applied.is_empty() {
                for a in &applied {
                    println!(
                        "  {} sign-off step applied: {} {} → {} ({})",
                        "✓".green(),
                        a.refdes,
                        a.from,
                        a.to,
                        a.note
                    );
                }
                // Re-package the stepped part(s) at their new value.
                let declared_v =
                    bhdl_synthesizer::glacier_physical_selection::declared_net_voltages(&netlist);
                let empty = std::collections::HashMap::new();
                bhdl_synthesizer::glacier_physical_selection::apply_catalog_physical_selection(
                    &mut netlist,
                    &harvest_catalog_families(),
                    &empty,
                    &empty,
                    &declared_v,
                );
            }
        }

        // Resolve real, orderable MPNs. Reproducibility model (mirrors
        // Cargo): a project's `bhdl.lock` (next to `bhdl.toml`) pins each
        // refdes→MPN. If pins exist and `--update-lock` was not given, reuse
        // them and DON'T call the provider; otherwise resolve via the
        // provider and write the pins back. `--locked` forbids resolving
        // (CI must build against committed pins).
        use bhdl_synthesizer::glacier_physical_selection as gps;
        let lock_path = source_path
            .parent()
            .and_then(bhdl_common::library::discover_project_manifest)
            .map(|m| m.with_file_name("bhdl.lock"));
        let existing_lock = lock_path
            .as_ref()
            .filter(|p| p.exists())
            .and_then(|p| bhdl_common::library::Lockfile::load(p).ok());
        let have_pins = existing_lock
            .as_ref()
            .map(|l| !l.parts.is_empty())
            .unwrap_or(false);

        if have_pins && !update_lock {
            let lock = existing_lock.as_ref().unwrap();
            let pins: Vec<gps::ResolvedPart> = lock
                .parts
                .iter()
                .map(|p| gps::ResolvedPart {
                    refdes: p.refdes.clone(),
                    mpn: p.mpn.clone(),
                    manufacturer: p.manufacturer.clone(),
                    vendor_sku: p.vendor_sku.clone(),
                    provider: p.provider.clone(),
                    esr_ohms: p.esr_ohms,
                    esr_test_freq_hz: p.esr_test_freq_hz,
                    dielectric: p.dielectric.clone(),
                    power_rating_w: p.power_rating_w,
                    voltage_rating_v: p.voltage_rating_v,
                    current_rating_a: p.current_rating_a,
                })
                .collect();
            let n = gps::apply_locked_parts(&mut netlist, &pins);
            if n > 0 {
                println!(
                    "  {} supply chain: {} MPN(s) pinned from bhdl.lock",
                    "🔒".green(),
                    n
                );
            }
        } else {
            if locked {
                anyhow::bail!(
                    "--locked: bhdl.lock has no part pins to build against; \
                     run once without --locked (or with --update-lock) to generate them"
                );
            }
            let supply_opts = gps::SupplyOptions {
                profile: supply_profile.clone(),
                quantity: supply_qty,
                net_profiles: gps::parse_net_profiles(&supply_net.join(",")),
            }
            .with_env_fallback();
            // Stress for the cap-voltage / resistor-power gates: simulated
            // node voltages + dissipation under --simulate, else declared
            // rail voltages (cap voltage works analytically; resistor power
            // needs the solve, so it only gates under --simulate).
            let empty_p = std::collections::HashMap::new();
            let declared_v = gps::declared_net_voltages(&netlist);
            let (gate_v, gate_p) = match &sim_stress {
                Some(ann) => (&ann.net_voltages, &ann.instance_power),
                None => (&declared_v, &empty_p),
            };
            let resolved =
                gps::apply_supply_chain_mpns(&mut netlist, &supply_opts, gate_v, gate_p);
            if !resolved.is_empty() {
                println!(
                    "  {} supply chain: {} real MPN(s) resolved",
                    "✓".green(),
                    resolved.len()
                );
                // Pin the selections so the next build is reproducible.
                if let Some(lp) = &lock_path {
                    let mut lock = existing_lock.unwrap_or(bhdl_common::library::Lockfile {
                        version: bhdl_common::library::Lockfile::CURRENT_VERSION,
                        libraries: Vec::new(),
                        parts: Vec::new(),
                    });
                    lock.set_parts(
                        resolved
                            .iter()
                            .map(|r| bhdl_common::library::LockedPart {
                                refdes: r.refdes.clone(),
                                mpn: r.mpn.clone(),
                                manufacturer: r.manufacturer.clone(),
                                vendor_sku: r.vendor_sku.clone(),
                                provider: r.provider.clone(),
                                esr_ohms: r.esr_ohms,
                                esr_test_freq_hz: r.esr_test_freq_hz,
                                dielectric: r.dielectric.clone(),
                                power_rating_w: r.power_rating_w,
                                voltage_rating_v: r.voltage_rating_v,
                                current_rating_a: r.current_rating_a,
                            })
                            .collect(),
                    );
                    match lock.save(lp) {
                        Ok(()) => println!(
                            "  {} wrote {} part pin(s) to {}",
                            "✓".green(),
                            resolved.len(),
                            lp.display()
                        ),
                        Err(e) => eprintln!("  warning: could not write bhdl.lock: {e}"),
                    }
                }
            }
        }
    }

    // 5. Walk the netlist; produce the BOM rows.
    let rows = sku_bom::walk(&netlist);
    println!("  {} {} BOM line(s) ({} total instance{})",
        "✓".green(),
        rows.len(),
        rows.iter().map(|r| r.quantity).sum::<usize>(),
        if rows.iter().map(|r| r.quantity).sum::<usize>() == 1 { "" } else { "s" });

    // 6. Format + emit.
    let text = match format {
        "csv" => sku_bom::to_csv(&rows),
        "markdown" | "md" => sku_bom::to_markdown(&rows),
        other => {
            anyhow::bail!("unknown BOM format '{other}' (expected 'markdown' or 'csv')");
        }
    };
    match output {
        Some(path) => {
            fs::write(&path, &text)?;
            println!("  Written to: {}", path.display());
        }
        None => {
            println!();
            print!("{}", text);
        }
    }

    // 7. Sign-off report (`--simulate`): re-solve the SNAPPED netlist and
    //    report each passive's margin (rating ÷ derated stress) against the
    //    catalogue rating the BOM selected. Spec: Simulation_Margin_Signoff.md.
    //    This is stage 4 — MEASURE + REPORT, no value changes; the stepping
    //    loop (#5) builds on the same margin computation. The earlier (4.55)
    //    solve runs on pre-snap values to seed the inductor gate; this one
    //    runs on the values that actually landed on the BOM.
    if simulate {
        let mut conv = NetlistToSpiceConverter::new();
        // §5 device-model surface: pre-evaluate any `simulation { model { } }`
        // blocks and hand the converter the resulting output-source voltages,
        // which override its hardcoded regulator decomposition.
        conv.set_model_overrides(bhdl_synthesizer::model_evaluator::evaluate_model_overrides(
            &netlist,
            &analysis.model_recipes,
            &analysis.entity_attribute_index,
        ));
        conv.set_ibis_models(
            analysis.model_recipes.iter()
                .filter_map(|(e, r)| (!r.ibis.is_empty()).then(|| (e.clone(), r.ibis.clone())))
                .collect(),
            std::env::current_dir().unwrap_or_default(),
        );
        match conv.convert(&netlist) {
            Ok(circuit) => {
                let circuit_ref = match bhdl_spice::input_draw::solve_dc_with_input_draws(circuit.clone(), &regulator_hints(&netlist)) {
                    Ok((_, final_circuit)) => final_circuit,
                    Err(_) => circuit.clone(),
                };
                match bhdl_spice::GlacierDcSolver::new().solve(circuit_ref.clone()) {
                    Ok(result) => {
                        let ann = build_simulation_annotations(&result, &circuit_ref);
                        // build_simulation_annotations prunes "internal"
                        // DC-equivalent nets from net_voltages for the
                        // schematic renderer — and a buck's VOUT rail is
                        // classed internal to its switch node, so its voltage
                        // is dropped. Sign-off needs it. Work on a clone and
                        // re-derive the pruned endpoint: an inductor is a DC
                        // short, so propagate the surviving (canonical) node's
                        // voltage across every DC-short branch (inductor, or
                        // sub-1Ω resistor — a fuse/shunt, the same criterion
                        // the pruning itself uses) back onto the pruned rail.
                        // Iterate to a fixpoint for chains.
                        let mut sv = ann.net_voltages.clone();
                        for _ in 0..8 {
                            let pairs: Vec<(String, String)> = circuit_ref
                                .branches()
                                .filter(|(_, b)| {
                                    b.component_type == "Inductor"
                                        || (b.component_type == "Resistor" && b.value < 1.0)
                                })
                                .filter_map(|(e, _)| {
                                    let (a, b) = circuit_ref.branch_nodes(e)?;
                                    Some((
                                        circuit_ref.get_node_name(a)?.to_string(),
                                        circuit_ref.get_node_name(b)?.to_string(),
                                    ))
                                })
                                .collect();
                            let mut changed = false;
                            for (na, nb) in pairs {
                                match (sv.get(&na).copied(), sv.get(&nb).copied()) {
                                    (Some(v), None) => {
                                        sv.insert(nb, v);
                                        changed = true;
                                    }
                                    (None, Some(v)) => {
                                        sv.insert(na, v);
                                        changed = true;
                                    }
                                    _ => {}
                                }
                            }
                            if !changed {
                                break;
                            }
                        }
                        // A regulator-driven output net has no inductor to
                        // re-derive across (LDOs): restore it directly from the
                        // decomposition's VOUT source branch (its value IS the
                        // net's DC voltage). Without this an LDO's output-side
                        // parts read "—" (UNCHECKED) despite a solved 5V rail.
                        for (e, b) in circuit_ref.branches() {
                            if b.component_type == "VoltageSource"
                                && b.metadata
                                    .get(bhdl_spice::META_DECOMPOSITION_ROLE)
                                    .map(|r| r.as_str())
                                    == Some("vout")
                            {
                                if let Some((a, _)) = circuit_ref.branch_nodes(e) {
                                    if let Some(name) = circuit_ref.get_node_name(a) {
                                        sv.entry(name.to_string()).or_insert(b.value);
                                    }
                                }
                            }
                        }
                        let rows = bhdl_synthesizer::signoff::compute_signoff(
                            &netlist,
                            &sv,
                            &ann.instance_power,
                            &ann.instance_currents,
                            &analysis.entity_attribute_index,
                            &analysis.stress_recipes,
                        );
                        if let Some(report) =
                            bhdl_synthesizer::signoff::format_signoff_report(&rows)
                        {
                            print!("{report}");
                        }
                        if let Some(derived) =
                            bhdl_synthesizer::signoff::format_derived_values(&netlist)
                        {
                            print!("{derived}");
                        }
                        // Control-loop stability (analytic, datasheet model),
                        // one assessment per regulator stage.
                        let stages = bhdl_synthesizer::signoff::compute_stability(
                            &netlist,
                            &sv,
                            &analysis.entity_attribute_index,
                        );
                        if let Some(stab) = bhdl_synthesizer::signoff::format_stability(&stages) {
                            print!("{stab}");
                        }
                    }
                    Err(e) => eprintln!(
                        "  {}",
                        format!("sign-off re-solve failed ({e}); margins not reported").yellow()
                    ),
                }
            }
            Err(e) => eprintln!(
                "  {}",
                format!("sign-off conversion failed ({e}); margins not reported").yellow()
            ),
        }
    }

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
    gate_constructor_args(&analysis)?;

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

/// Default current threshold for power vs signal net classification.
/// Nets with branch current >= 1mA are classified as power nets (rendered red).
const POWER_THRESHOLD_AMPS: f64 = 1e-3;

/// Map GLACIER DC solver results (NodeIndex/EdgeIndex) back to schematic-level
/// Netlist-exact regulator hints for the input-draw fixpoint: input net,
/// output net (VIRTUAL pins count — a buck's VOUT is its logical output),
/// owning instance, and datasheet efficiency.
fn regulator_hints(
    netlist: &bhdl_netlist::netlist::Netlist,
) -> std::collections::HashMap<String, bhdl_spice::input_draw::RegulatorHint> {
    let mut out = std::collections::HashMap::new();
    for (iid, inst) in &netlist.instances {
        let class = inst
            .attributes
            .get("component_class")
            .or_else(|| {
                netlist
                    .modules
                    .get(inst.definition)
                    .and_then(|m| m.attributes.get("component_class"))
            })
            .map(String::as_str)
            .unwrap_or("");
        if !matches!(class, "voltage_regulator" | "ldo" | "switching_regulator") {
            continue;
        }
        let net_of = |pred: &dyn Fn(&bhdl_netlist::portpin::Pin) -> bool| {
            netlist.pin_instances.values().find_map(|pi| {
                (pi.instance == iid)
                    .then(|| netlist.pins.get(pi.pin_def))
                    .flatten()
                    .filter(|p| pred(p))
                    .and_then(|_| pi.net)
                    .and_then(|nid| netlist.nets.get(nid))
                    .and_then(|n| n.name.clone())
            })
        };
        // Pin TYPES first (`power in` lowers to PinDirection::Power — the
        // datasheet truth stdlib entities declare), pin names second (VI is
        // what the NCP1117 datasheet calls its input; VIN what most others do).
        let vin = net_of(&|p| {
            p.pin_type == bhdl_netlist::types::PinType::Power
                && p.direction == bhdl_netlist::types::PinDirection::Power
        })
        .or_else(|| {
            net_of(&|p| {
                p.name.eq_ignore_ascii_case("VIN")
                    || p.name.eq_ignore_ascii_case("IN")
                    || p.name.eq_ignore_ascii_case("VI")
            })
        });
        // The output rail. Normally the VOUT pin's net — but when the board
        // hand-authors the application circuit, the (virtual) VOUT pin is
        // unwired (its expansion skipped, see expansion_interpreter's
        // board-authored takeover), so fall back to the rail the regulator
        // actually drives: trace from the switch/output pin through the
        // board's series parts (inductor) to a Power-class net.
        let vout = net_of(&|p| {
            p.pin_type == bhdl_netlist::types::PinType::Power
                && p.direction == bhdl_netlist::types::PinDirection::Out
                && !p.is_virtual
        })
        .or_else(|| {
            net_of(&|p| {
                p.name.eq_ignore_ascii_case("VOUT")
                    || p.name.eq_ignore_ascii_case("OUT")
                    || p.name.eq_ignore_ascii_case("VO")
            })
        })
        .or_else(|| {
            let traced = driven_output_rail(netlist, iid);
            if let Some(rail) = &traced {
                info!(
                    "regulator hint for '{}': VOUT pin unwired (board-authored \
                     output path) — traced the driven rail '{}' through the \
                     board's series parts",
                    inst.name, rail
                );
            }
            traced
        });
        let eff = inst
            .attributes
            .get("efficiency")
            .and_then(|e| e.trim().trim_end_matches('%').trim().parse::<f64>().ok())
            .map(|e| if e > 1.0 { e / 100.0 } else { e });
        if let (Some(vin_net), Some(vout_net)) = (vin, vout) {
            out.insert(
                inst.name.clone(),
                bhdl_spice::input_draw::RegulatorHint {
                    vin_net,
                    vout_net,
                    instance: iid,
                    efficiency: eff,
                },
            );
        }
    }
    out
}

/// The Power-class rail a regulator drives THROUGH board-authored series
/// parts, for when its VOUT pin has no net (virtual pin, expansion skipped
/// because the board hand-authored the application circuit). Breadth-first
/// from the regulator's output-direction pins (the buck's SW node), crossing
/// only two-pin series passives (inductor/resistor/ferrite) authored on the
/// board, at most two hops — enough for SW → L → rail without wandering the
/// whole board. Returns the first Power-class net name reached.
fn driven_output_rail(
    netlist: &bhdl_netlist::netlist::Netlist,
    iid: bhdl_netlist::InstanceId,
) -> Option<String> {
    use bhdl_netlist::types::PinDirection;
    // Pin-instance membership per net is scanned repeatedly; a regulator has
    // a handful of pins and we visit ≤ a dozen nets, so linear scans are fine.
    let start_nets: Vec<bhdl_netlist::NetId> = netlist
        .pin_instances
        .values()
        .filter(|pi| pi.instance == iid)
        .filter(|pi| {
            netlist
                .pins
                .get(pi.pin_def)
                .map(|p| matches!(p.direction, PinDirection::Out))
                .unwrap_or(false)
        })
        .filter_map(|pi| pi.net)
        .collect();

    let series_hop = |nid: bhdl_netlist::NetId| -> Vec<bhdl_netlist::NetId> {
        let mut next = Vec::new();
        for pi in netlist.pin_instances.values() {
            if pi.net != Some(nid) || pi.instance == iid {
                continue;
            }
            let Some(inst) = netlist.instances.get(pi.instance) else { continue };
            let class = inst
                .attributes
                .get("component_class")
                .or_else(|| {
                    netlist
                        .modules
                        .get(inst.definition)
                        .and_then(|m| m.attributes.get("component_class"))
                })
                .map(String::as_str)
                .unwrap_or("");
            if !matches!(class, "inductor" | "resistor" | "ferrite" | "ferrite_bead") {
                continue;
            }
            // The series part's OTHER pin's net.
            for pi2 in netlist.pin_instances.values() {
                if pi2.instance == pi.instance && pi2.id != pi.id {
                    if let Some(n2) = pi2.net {
                        if n2 != nid {
                            next.push(n2);
                        }
                    }
                }
            }
        }
        next
    };

    let mut frontier = start_nets;
    let mut seen: std::collections::HashSet<bhdl_netlist::NetId> =
        frontier.iter().copied().collect();
    for _hop in 0..2 {
        let mut next_frontier = Vec::new();
        for nid in frontier.drain(..) {
            for n2 in series_hop(nid) {
                if !seen.insert(n2) {
                    continue;
                }
                if let Some(net) = netlist.nets.get(n2) {
                    if matches!(net.net_class, bhdl_netlist::NetClass::Power { .. }) {
                        return net.name.clone();
                    }
                }
                next_frontier.push(n2);
            }
        }
        frontier = next_frontier;
    }
    None
}

/// EXACT block-port currents: for every hierarchical sheet group, the net
/// injection of the group's branches into each net — the physical current
/// crossing the block boundary, no dominant-carrier heuristics. Keyed
/// "parent::net".
fn compute_port_currents(
    dc: &bhdl_spice::DcAnalysisResult,
    circuit: &bhdl_spice::circuit::Circuit,
    netlist: &bhdl_netlist::netlist::Netlist,
) -> std::collections::HashMap<String, f64> {
    let mut out = std::collections::HashMap::new();
    let Some(groups) = bhdl_schematic::v4::partition_sheets(netlist) else {
        return out;
    };
    let hints = regulator_hints(netlist);
    for g in &groups {
        let Some(parent) = &g.parent else { continue };
        // Nets DRIVEN by this group's parent (its output pins): the board
        // rail's own ideal source on such a net electrically IS this
        // regulator's output stage (double-drive prevention leaves exactly
        // one source) and must count as a group member, or the source and
        // the downstream draws cancel inside the complement.
        let driven: std::collections::HashSet<&str> = hints
            .get(parent)
            .map(|h| std::iter::once(h.vout_net.as_str()).collect())
            .unwrap_or_default();
        // Group membership by instance NAME (branches carry instance ids;
        // synthesized branches like "{inst}_draw"/"_vout" carry the
        // parent's id, so id→name→group is exact).
        let member_names: std::collections::HashSet<String> = g
            .members
            .iter()
            .filter_map(|id| netlist.instances.get(*id).map(|i| i.name.clone()))
            .collect();
        // Signed injection per net, member side and complement side. By
        // KCL they are equal and opposite when every branch current is
        // known; a CLAMPED source (the solver pins the node instead of
        // modelling a source branch) hides its current from one side, so
        // the side with the LARGER magnitude is the complete one.
        let mut inj_member: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let mut inj_other: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let is_member = |b: &bhdl_spice::circuit::Branch| -> bool {
            b.instance_id
                .and_then(|id| netlist.instances.get(id))
                .map(|i| member_names.contains(&i.name))
                .unwrap_or(false)
                || b.metadata
                    .get(bhdl_spice::circuit::META_PARENT_INSTANCE)
                    .map(|p| member_names.contains(p))
                    .unwrap_or(false)
                || (b.component_type == "VoltageSource"
                    && b.instance_id.is_none()
                    && b.nodes
                        .iter()
                        .filter_map(|&n| circuit.get_node_name(n))
                        .any(|n| driven.contains(n)))
        };
        // At most ONE member VoltageSource per net: on a declared rail that
        // a regulator drives, the rail's own source and the decomposed
        // regulator source can BOTH exist; counting both doubles the port
        // current (buck_converter_simple read 4A into a 2A load).
        let mut source_counted: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (edge, b) in circuit.branches() {
            if b.nodes.len() != 2 {
                continue;
            }
            // Membership by instance id OR by parent metadata — decomposed
            // regulator branches (vout source, dropout) carry the parent in
            // METADATA; missing them puts the source in the complement,
            // where it cancels against the very draw it supplies.
            let member = is_member(b);
            if member && b.component_type == "VoltageSource" {
                let key = b
                    .nodes
                    .iter()
                    .filter_map(|&n| circuit.get_node_name(n))
                    .collect::<Vec<_>>()
                    .join("|");
                if !source_counted.insert(key) {
                    continue; // second parallel source on the same net
                }
            }
            let mut i = dc.branch_currents.get(&edge).copied().unwrap_or_else(|| {
                if b.component_type == "CurrentSource" { b.value } else { 0.0 }
            });
            // The solver reports SOURCE currents in the delivery convention
            // (positive = out of the + terminal), passives in the
            // nodes[0]→nodes[1] convention — mixing them made a source and
            // its load ADD in the injection sum instead of cancel (a 2A
            // load read as a 4A port). Normalize sources to the passive
            // convention.
            if b.component_type == "VoltageSource" {
                i = -i;
            }
            let sink = if member { &mut inj_member } else { &mut inj_other };
            let name_of = |n| circuit.get_node_name(n).map(str::to_string);
            if let Some(a) = name_of(b.nodes[0]) {
                *sink.entry(a).or_insert(0.0) -= i;
            }
            if let Some(bn) = name_of(b.nodes[1]) {
                *sink.entry(bn).or_insert(0.0) += i;
            }
        }
        // Only report nets the group actually touches.
        let touched: std::collections::HashSet<String> = circuit
            .branches()
            .filter(|(_, b)| is_member(b))
            .flat_map(|(_, b)| b.nodes.iter().filter_map(|&n| circuit.get_node_name(n).map(str::to_string)).collect::<Vec<_>>())
            .collect();
        for net in touched {
            let m = inj_member.get(&net).copied().unwrap_or(0.0).abs();
            let o = inj_other.get(&net).copied().unwrap_or(0.0).abs();
            let i = m.max(o);
            if std::env::var("BHDL_V4_DEBUG").is_ok() {
                eprintln!("[v4] port {parent}::{net} member={m:.6} other={o:.6}");
            }
            if i >= 1e-6 {
                out.insert(format!("{parent}::{net}"), i);
            }
        }
    }
    out
}

/// Stimulus-response experiment over the sheet's signal chain (task #41):
/// drive a 100 mV / 1 kHz sine at the chain's input net through the linear
/// transient solver (which stamps the ideal op-amp rows) and MEASURE the
/// output amplitude over the final cycle. Returns None when the sheet has
/// no op-amp chain or the transient fails — an absent annotation, never a
/// fabricated one.
fn run_chain_stimulus(
    netlist: &bhdl_netlist::netlist::Netlist,
    circuit: &bhdl_spice::circuit::Circuit,
) -> Option<bhdl_schematic::StimulusResponse> {
    let plan = bhdl_schematic::v4::classify_sheet(netlist);
    let chain = plan.chains.first()?;
    let net_name =
        |id: bhdl_netlist::types::NetId| netlist.nets.get(id).and_then(|n| n.name.clone());
    let input_net: String = net_name(*chain.spine_nets.first()?)?;
    let output_net: String = net_name(*chain.spine_nets.last()?)?;

    // Stage probes: pins the parts THEMSELVES declared via
    // `attribute sim_probe = "<pin>"` (stdlib policy — when to measure a
    // stage and where, decided by the part, not the renderer).
    let chain_insts: Vec<&str> = chain
        .elems
        .iter()
        .filter_map(|e| match e {
            bhdl_schematic::v4::classify::ChainElem::Amp { inst, .. } => Some(inst.as_str()),
            _ => None,
        })
        .collect();
    let mut stage_probes: Vec<(String, String)> = Vec::new(); // (instance, net)
    for inst_name in &chain_insts {
        let Some((iid, inst)) = netlist.instances.iter().find(|(_, i)| i.name == *inst_name)
        else {
            continue;
        };
        let probe_pin = inst
            .attributes
            .get("sim_probe")
            .or_else(|| {
                netlist
                    .modules
                    .get(inst.definition)
                    .and_then(|m| m.attributes.get("sim_probe"))
            })
            .cloned();
        let Some(pin) = probe_pin else { continue };
        let pin = pin.trim_matches('"').to_string();
        let net = netlist.pin_instances.values().find_map(|pi| {
            (pi.instance == iid
                && netlist
                    .pins
                    .get(pi.pin_def)
                    .map(|d| d.name.eq_ignore_ascii_case(&pin))
                    .unwrap_or(false))
                .then_some(pi.net)
                .flatten()
        });
        if let Some(nname) = net.and_then(net_name) {
            stage_probes.push((inst_name.to_string(), nname));
        }
    }

    const AMP: f64 = 0.1; // 100 mV — inside the rails at any sane gain
    const FREQ: f64 = 1_000.0;
    let mut probes = vec![input_net.clone(), output_net.clone()];
    for (_, n) in &stage_probes {
        if !probes.contains(n) {
            probes.push(n.clone());
        }
    }
    let params = bhdl_spice::transient::TransientParams::new(
        input_net.clone(),
        bhdl_spice::transient::Stimulus::Sine {
            amplitude: AMP,
            frequency_hz: FREQ,
            dc_offset: 0.0,
        },
        probes,
        5.0 / FREQ,       // five cycles — settle, then measure the last
        1.0 / FREQ / 200.0, // 200 points per cycle
    );
    let result = match bhdl_spice::transient::run_transient(circuit, &params) {
        Ok(r) => r,
        Err(e) => {
            info!("chain stimulus transient failed: {e} — no waveform annotation");
            return None;
        }
    };
    let vout = result.probe_voltages.get(&output_net)?;
    if vout.is_empty() {
        return None;
    }
    // Final cycle = last 200 samples.
    let tail = &vout[vout.len().saturating_sub(200)..];
    let max = tail.iter().cloned().fold(f64::MIN, f64::max);
    let min = tail.iter().cloned().fold(f64::MAX, f64::min);
    let vout_amplitude = (max - min) / 2.0;
    // Clipped = the measured extremes sit ON an op-amp rail (within 1 mV).
    let clipped = circuit.branches().any(|(_, b)| {
        b.component_type == "OpAmp"
            && [
                (bhdl_spice::circuit::META_VSAT_P, max),
                (bhdl_spice::circuit::META_VSAT_N, min),
            ]
            .iter()
            .any(|(key, v)| {
                b.metadata
                    .get(*key)
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|rail| (v - rail).abs() < 1e-3)
                    .unwrap_or(false)
            })
    });
    // Per-stage measurements at the declared probe nets, clipping judged
    // against EACH instance's own supply rails.
    let rails_of = |inst_name: &str| -> (Option<f64>, Option<f64>) {
        let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == inst_name)
        else {
            return (None, None);
        };
        let rail = |pin: &str| {
            netlist.pin_instances.values().find_map(|pi| {
                (pi.instance == iid
                    && netlist
                        .pins
                        .get(pi.pin_def)
                        .map(|d| d.name.eq_ignore_ascii_case(pin))
                        .unwrap_or(false))
                    .then_some(pi.net)
                    .flatten()
                    .and_then(|nid| netlist.nets.get(nid))
                    .and_then(|n| match n.net_class {
                        bhdl_netlist::types::NetClass::Power { voltage, .. } => Some(voltage),
                        _ => None,
                    })
            })
        };
        (rail("VCC"), rail("VEE"))
    };
    let stages: Vec<bhdl_schematic::StageResponse> = stage_probes
        .iter()
        .filter_map(|(inst, nname)| {
            let v = result.probe_voltages.get(nname)?;
            let tail = &v[v.len().saturating_sub(200)..];
            let smax = tail.iter().cloned().fold(f64::MIN, f64::max);
            let smin = tail.iter().cloned().fold(f64::MAX, f64::min);
            let (vp, vn) = rails_of(inst);
            let stage_clipped = vp.map(|r| (smax - r).abs() < 1e-3).unwrap_or(false)
                || vn.map(|r| (smin - r).abs() < 1e-3).unwrap_or(false);
            Some(bhdl_schematic::StageResponse {
                instance: inst.clone(),
                net: nname.clone(),
                amplitude: (smax - smin) / 2.0,
                clipped: stage_clipped,
            })
        })
        .collect();
    info!(
        "chain stimulus: {AMP} V @ {FREQ} Hz at {input_net} -> {vout_amplitude:.4} V at {output_net}{}; stages: {}",
        if clipped { " (CLIPPED)" } else { "" },
        stages
            .iter()
            .map(|s| format!("{}={:.4}V{}", s.instance, s.amplitude, if s.clipped { "!" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Some(bhdl_schematic::StimulusResponse {
        input_net,
        output_net,
        frequency_hz: FREQ,
        vin_amplitude: AMP,
        vout_amplitude,
        clipped,
        stages,
    })
}

/// net names and instance names, producing a `SimulationAnnotations` struct
/// that the JS renderer can consume for wire coloring and hover annotations.
fn build_simulation_annotations(
    dc_result: &bhdl_spice::DcAnalysisResult,
    circuit: &bhdl_spice::Circuit,
) -> bhdl_schematic::SimulationAnnotations {
    let mut annotations = bhdl_schematic::SimulationAnnotations::default();

    // Map node voltages: NodeIndex → node.name → voltage
    for (node_idx, voltage) in &dc_result.node_voltages {
        if let Some(name) = circuit.get_node_name(*node_idx) {
            annotations.net_voltages.insert(name.to_string(), *voltage);
        }
    }
    // The reference node is implicit at 0 V; make it explicit so a part
    // bridging a live net and ground resolves two endpoints, not one.
    annotations.net_voltages.entry("GND".to_string()).or_insert(0.0);

    // Map branch currents: EdgeIndex → branch.name → current
    // Also compute power dissipation per branch
    for (edge_idx, current) in &dc_result.branch_currents {
        if let Some(branch) = circuit.graph.edge_weight(*edge_idx) {
            annotations.instance_currents.insert(branch.name.clone(), *current);

            // Power = |V_across| * |I|
            if let Some((src, tgt)) = circuit.branch_nodes(*edge_idx) {
                let v_src = dc_result.node_voltages.get(&src).unwrap_or(&0.0);
                let v_tgt = dc_result.node_voltages.get(&tgt).unwrap_or(&0.0);
                let power = (v_src - v_tgt).abs() * current.abs();
                annotations.instance_power.insert(branch.name.clone(), power);
            }
        }
    }

    // CurrentSource branches (model-declared loads) carry their DEFINED
    // current — the solver has nothing to compute for them, so they never
    // appear in branch_currents. Without this the one branch a load block
    // exists to annotate is the one with no number.
    for (_, branch) in circuit.branches() {
        if branch.component_type == "CurrentSource" {
            annotations
                .instance_currents
                .entry(branch.name.clone())
                .or_insert(branch.value);
        }
    }

    // Classify power nets: a net is "power" if any branch connected to it carries
    // current above the threshold. We iterate all branches and mark both endpoint
    // nets as power when the branch current is significant.
    for (edge_idx, current) in &dc_result.branch_currents {
        if current.abs() >= POWER_THRESHOLD_AMPS {
            if let Some((src, tgt)) = circuit.branch_nodes(*edge_idx) {
                if let Some(name) = circuit.get_node_name(src) {
                    annotations.power_nets.insert(name.to_string());
                }
                if let Some(name) = circuit.get_node_name(tgt) {
                    annotations.power_nets.insert(name.to_string());
                }
            }
        }
    }

    // Never classify GND as a power net for rendering purposes (it gets its own gray color)
    annotations.power_nets.remove("GND");
    annotations.power_nets.remove("0");

    // Unify regulator decomposition and cascade currents.
    //
    // GLACIER decomposes each regulator into two branches with structured
    // metadata (parent_instance + decomposition_role) rather than relying
    // on name suffixes. The voltage source independently sources load
    // current from GND, so downstream regulator loads don't appear at the
    // upstream VIN — violating KCL for annotation purposes.
    //
    // Fix: collect each regulator's VOUT current via metadata, then cascade
    // bottom-up so that an upstream regulator's current includes all
    // downstream loads.

    // 1. Collect regulator info from circuit graph using structured metadata
    struct RegInfo {
        base_name: String,
        vout_current: f64,
        vout_node: String,      // VOUT net name
        vin_node: String,       // VIN net name
        is_switching: bool,     // switching_regulator vs linear
        // Device loss model parameters (from datasheet, stored in metadata)
        i_quiescent: f64,       // regulator quiescent current (A) — applies to all types
        rds_on: f64,            // MOSFET on-resistance (Ω) — switching only
        f_sw: f64,              // switching frequency (Hz) — switching only
        t_sw: f64,              // switching transition time (s) — switching only
        // §5 model surface: vendor-authored input current (A) from a
        // `model { node VIN draws = … }` block. `Some` ⇒ supersede the physics
        // loss model with the efficiency model (P_in − P_out).
        model_i_in: Option<f64>,
    }
    let mut regulators: Vec<RegInfo> = Vec::new();

    let read_meta_f64 = |branch: &bhdl_spice::Branch, key: &str, default: f64| -> f64 {
        branch.metadata.get(key)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default)
    };

    for (edge_idx, current) in &dc_result.branch_currents {
        if let Some(branch) = circuit.graph.edge_weight(*edge_idx) {
            let is_vout = branch.metadata
                .get(bhdl_spice::META_DECOMPOSITION_ROLE)
                .map(|r| r.as_str()) == Some("vout");
            if is_vout {
                let base = branch.metadata
                    .get(bhdl_spice::META_PARENT_INSTANCE)
                    .cloned()
                    .unwrap_or_default();

                let component_class = branch.metadata
                    .get(bhdl_spice::META_COMPONENT_CLASS)
                    .cloned()
                    .unwrap_or_default();
                let is_switching = component_class == "switching_regulator";

                // _vout branch connects VOUT → GND
                if let Some((src, _tgt)) = circuit.branch_nodes(*edge_idx) {
                    let vout_node = circuit.get_node_name(src)
                        .unwrap_or("").to_string();

                    // Find the matching dropout branch by metadata
                    let vin_node = dc_result.branch_currents.keys()
                        .filter_map(|eidx| {
                            let b = circuit.graph.edge_weight(*eidx)?;
                            if b.metadata.get(bhdl_spice::META_PARENT_INSTANCE).map(|s| s.as_str()) == Some(&base)
                                && b.metadata.get(bhdl_spice::META_DECOMPOSITION_ROLE).map(|s| s.as_str()) == Some("dropout")
                            {
                                let (s, _) = circuit.branch_nodes(*eidx)?;
                                circuit.get_node_name(s).map(|n| n.to_string())
                            } else {
                                None
                            }
                        })
                        .next()
                        .unwrap_or_default();

                    regulators.push(RegInfo {
                        base_name: base,
                        vout_current: current.abs(),
                        vout_node,
                        vin_node,
                        is_switching,
                        rds_on: read_meta_f64(branch, bhdl_spice::META_RDS_ON, 0.2),
                        f_sw: read_meta_f64(branch, bhdl_spice::META_F_SW, 500e3),
                        t_sw: read_meta_f64(branch, bhdl_spice::META_T_SW, 80e-9),
                        i_quiescent: read_meta_f64(branch, bhdl_spice::META_I_QUIESCENT, 5e-3),
                        model_i_in: branch.metadata.get(bhdl_spice::META_MODEL_I_IN)
                            .and_then(|s| s.parse::<f64>().ok()),
                    });
                }
            }
        }
    }

    // 1b. Build DC node equivalence classes.
    //     At DC, inductors are short circuits (modeled with small DCR in GLACIER).
    //     Any branch with impedance below a threshold creates a DC-equivalent pair.
    //     This lets the cascade match through internal switching nodes (e.g.
    //     buck_sw ≡ V5_BUCK via inductor DCR) without special-casing component types.
    const DC_SHORT_THRESHOLD: f64 = 1.0; // Ω — branches below this are DC shorts
    let mut dc_equiv: HashMap<String, String> = HashMap::new(); // node → canonical representative
    for edge in circuit.graph.edge_indices() {
        if let Some(branch) = circuit.graph.edge_weight(edge) {
            let is_dc_short = match branch.component_type.as_str() {
                "Inductor" => true, // always a DC short regardless of modeled DCR
                "Resistor" => branch.value < DC_SHORT_THRESHOLD,
                _ => false,
            };
            if is_dc_short {
                if let Some((src, tgt)) = circuit.branch_nodes(edge) {
                    let src_name = circuit.get_node_name(src).unwrap_or("").to_string();
                    let tgt_name = circuit.get_node_name(tgt).unwrap_or("").to_string();
                    if !src_name.is_empty() && !tgt_name.is_empty()
                        && src_name != "GND" && src_name != "0"
                        && tgt_name != "GND" && tgt_name != "0"
                    {
                        // Union: both map to the same canonical name.
                        // Prefer the name that doesn't look like an internal node (no '_sw' suffix).
                        let canonical = if src_name.ends_with("_sw") || src_name.ends_with("_SW") {
                            &tgt_name
                        } else {
                            &src_name
                        };
                        let resolve = |n: &str| dc_equiv.get(n).cloned().unwrap_or_else(|| n.to_string());
                        let canon = resolve(canonical);
                        dc_equiv.insert(src_name.clone(), canon.clone());
                        dc_equiv.insert(tgt_name.clone(), canon.clone());
                        info!("DC equivalence: {} ≡ {} (via {} {:.3}Ω)",
                              src_name, tgt_name, branch.component_type, branch.value);
                    }
                }
            }
        }
    }
    // Resolve each regulator's vout_node/vin_node to canonical form so that
    // all downstream code (cascade, power symbol propagation, voltage lookup)
    // uses the user-visible net names.
    let dc_resolve = |node: &str| -> String {
        dc_equiv.get(node).cloned().unwrap_or_else(|| node.to_string())
    };
    for reg in &mut regulators {
        let resolved = dc_resolve(&reg.vout_node);
        if resolved != reg.vout_node {
            info!("Cascade: {} vout_node {} → {} (DC equivalent)", reg.base_name, reg.vout_node, resolved);
            reg.vout_node = resolved;
        }
        let resolved = dc_resolve(&reg.vin_node);
        if resolved != reg.vin_node {
            info!("Cascade: {} vin_node {} → {} (DC equivalent)", reg.base_name, reg.vin_node, resolved);
            reg.vin_node = resolved;
        }
    }

    // 2. Cascade: a regulator's true current = its own vout_current +
    //    sum of downstream regulators whose VIN is on this regulator's VOUT.
    //    Process iteratively until stable (handles arbitrary cascade depth).
    let mut reg_currents: HashMap<String, f64> = regulators.iter()
        .map(|r| (r.base_name.clone(), r.vout_current))
        .collect();

    for _ in 0..regulators.len() {
        let snapshot = reg_currents.clone();
        for reg in &regulators {
            let downstream_sum: f64 = regulators.iter()
                .filter(|d| d.vin_node == reg.vout_node && d.base_name != reg.base_name)
                .map(|d| snapshot.get(&d.base_name).copied().unwrap_or(0.0))
                .sum();
            reg_currents.insert(
                reg.base_name.clone(),
                reg.vout_current + downstream_sum,
            );
        }
    }

    // 2b. Propagate cascaded regulator currents back to power symbols.
    // Power symbols (voltage sources) in GLACIER only see tiny dropout-resistor
    // current. The actual current they source = sum of all top-level regulators
    // whose VIN connects to that power net.
    {
        // Collect all power net names that regulators draw from
        let mut power_net_current: HashMap<String, f64> = HashMap::new();
        for reg in &regulators {
            let current = reg_currents.get(&reg.base_name).copied().unwrap_or(reg.vout_current);
            // Only count regulators whose VIN is a power net (top-level feed),
            // not regulators cascading from another regulator's VOUT.
            let fed_by_regulator = regulators.iter().any(|r| r.vout_node == reg.vin_node);
            if !fed_by_regulator {
                *power_net_current.entry(reg.vin_node.clone()).or_insert(0.0) += current;
            }
        }
        // Update power symbol instance currents
        for (net_name, total_current) in &power_net_current {
            // Power symbol instance name typically matches the net name
            if annotations.instance_currents.contains_key(net_name) {
                annotations.instance_currents.insert(net_name.clone(), *total_current);
                // Power symbol dissipates nothing (ideal source)
                annotations.instance_power.insert(net_name.clone(), 0.0);
            }
        }
    }

    // 3. Write unified entries and remove decomposed ones (found by metadata scan)
    for reg in &regulators {
        let current = reg_currents.get(&reg.base_name).copied().unwrap_or(reg.vout_current);
        annotations.instance_currents.insert(reg.base_name.clone(), current);

        // Find all decomposed branch names for this regulator by metadata
        let decomposed_keys: Vec<String> = dc_result.branch_currents.keys()
            .filter_map(|eidx| {
                let b = circuit.graph.edge_weight(*eidx)?;
                if b.metadata.get(bhdl_spice::META_PARENT_INSTANCE).map(|s| s.as_str()) == Some(&reg.base_name) {
                    Some(b.name.clone())
                } else {
                    None
                }
            })
            .collect();

        // Regulator power dissipation from device parameters + simulation
        // operating point. Both types include quiescent current draw.
        //
        // LINEAR:
        //   P_pass      = (V_IN - V_OUT) × I_load   (pass transistor heat)
        //   P_quiescent = V_IN × I_q                 (internal bias circuits)
        //   P_total     = P_pass + P_quiescent
        //
        // SWITCHING:
        //   P_conduction = I_OUT² × Rds_on × D       (MOSFET resistive loss)
        //   P_switching  = V_IN × I_OUT × f_sw × t_sw / 2  (transition loss)
        //   P_quiescent  = V_IN × I_q                (controller self-consumption)
        //   P_total      = P_conduction + P_switching + P_quiescent
        //   (diode and inductor losses are separate components)
        let v_in = annotations.net_voltages.get(&reg.vin_node).copied().unwrap_or(0.0);
        let v_out = annotations.net_voltages.get(&reg.vout_node).copied().unwrap_or(0.0);
        let p_quiescent = v_in * reg.i_quiescent;
        let total_power = if let Some(i_in) = reg.model_i_in {
            // §5: the entity authored its input current (efficiency model). It
            // supersedes the generic physics loss model — the vendor's
            // datasheet-specific correction. Regulator loss = P_in − P_out
            // (the efficiency already accounts for conduction/switching/bias).
            (i_in * v_in - current * v_out).max(0.0)
        } else if reg.is_switching && v_in > 0.0 {
            let d = v_out / v_in;  // duty cycle (CCM)
            let p_conduction = current * current * reg.rds_on * d;
            let p_switching = v_in * current * reg.f_sw * reg.t_sw / 2.0;
            p_conduction + p_switching + p_quiescent
        } else {
            let p_pass = (v_in - v_out).abs() * current;
            p_pass + p_quiescent
        };
        annotations.instance_power.insert(reg.base_name.clone(), total_power);

        // Remove decomposed entries
        for key in &decomposed_keys {
            annotations.instance_currents.remove(key);
            annotations.instance_power.remove(key);
        }

        // Also set current on the VOUT power symbol if it exists.
        // When GLACIER skips a power symbol (net already regulator-driven),
        // the power symbol has no instance_currents entry but the schematic
        // renderer uses it as the net driver for current annotations.
        if !annotations.instance_currents.contains_key(&reg.vout_node) {
            // Check if there's actually a power symbol for this net
            // (power symbols have the same name as the net they drive)
            if annotations.net_voltages.contains_key(&reg.vout_node) {
                annotations.instance_currents.insert(reg.vout_node.clone(), current);
                annotations.instance_power.insert(reg.vout_node.clone(), 0.0);
            }
        }
    }

    // 4. Update instance currents for DC-short components (inductors, 0Ω jumpers).
    //    These components bridge two DC-equivalent nodes.  Their raw GLACIER current
    //    only reflects the loads on the far side, missing cascaded regulator loads.
    //    Replace with the regulator's cascaded current since the component is just
    //    a DC pass-through.
    for edge in circuit.graph.edge_indices() {
        if let Some(branch) = circuit.graph.edge_weight(edge) {
            let is_dc_short = match branch.component_type.as_str() {
                "Inductor" => true,
                "Resistor" => branch.value < DC_SHORT_THRESHOLD,
                _ => false,
            };
            if !is_dc_short { continue; }
            if let Some((src, tgt)) = circuit.branch_nodes(edge) {
                let src_name = circuit.get_node_name(src).unwrap_or("").to_string();
                let tgt_name = circuit.get_node_name(tgt).unwrap_or("").to_string();
                // Check if either end is a regulator's vout_node
                for reg in &regulators {
                    if reg.vout_node == src_name || reg.vout_node == tgt_name {
                        let cascaded = reg_currents.get(&reg.base_name).copied()
                            .unwrap_or(reg.vout_current);
                        if let Some(inst_name) = &branch.instance_id {
                            // Find the instance name from the branch
                            let name = branch.name.clone();
                            if annotations.instance_currents.contains_key(&name) {
                                info!("DC-short {} current updated: {:.3}mA → {:.3}mA (cascaded from {})",
                                      name, annotations.instance_currents[&name] * 1000.0,
                                      cascaded * 1000.0, reg.base_name);
                                annotations.instance_currents.insert(name, cascaded);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // 5. Mark internal DC-equivalent nets so the renderer can suppress their
    //    annotations.  Internal nets (e.g. buck_sw) are GLACIER artifacts from
    //    virtual pin expansion; the canonical net (V5_BUCK) carries the annotations.
    for (internal, canonical) in &dc_equiv {
        if internal != canonical {
            annotations.net_voltages.remove(internal);
            annotations.power_nets.remove(internal);
            annotations.internal_nets.insert(internal.clone());
        }
    }

    annotations
}