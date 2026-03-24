//! Integration test: Full BHDL pipeline → PnR Board.
//!
//! Run: cargo run -p bhdl-pnr --bin test_semantic [circuit.bhdl]
//!
//! Default circuit: tests/circuits/simple/complex_power_tree.bhdl

use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_spice::NetlistToSpiceConverter;

use bhdl_pnr::semantic::{self, SemanticConfig};
use bhdl_pnr::ipc7351;

// ---------------------------------------------------------------------------
// build_simulation_annotations (mirrors bhdl-cli)
// ---------------------------------------------------------------------------

const POWER_THRESHOLD_AMPS: f64 = 1e-3;

fn build_simulation_annotations(
    dc_result: &bhdl_spice::DcAnalysisResult,
    circuit: &bhdl_spice::Circuit,
) -> bhdl_schematic::SimulationAnnotations {
    let mut annotations = bhdl_schematic::SimulationAnnotations::default();

    for (node_idx, voltage) in &dc_result.node_voltages {
        if let Some(name) = circuit.get_node_name(*node_idx) {
            annotations.net_voltages.insert(name.to_string(), *voltage);
        }
    }

    for (edge_idx, current) in &dc_result.branch_currents {
        if let Some(branch) = circuit.graph.edge_weight(*edge_idx) {
            annotations
                .instance_currents
                .insert(branch.name.clone(), *current);

            if let Some((src, tgt)) = circuit.branch_nodes(*edge_idx) {
                let v_src = dc_result.node_voltages.get(&src).unwrap_or(&0.0);
                let v_tgt = dc_result.node_voltages.get(&tgt).unwrap_or(&0.0);
                let power = (v_src - v_tgt).abs() * current.abs();
                annotations
                    .instance_power
                    .insert(branch.name.clone(), power);
            }
        }
    }

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
    annotations.power_nets.remove("GND");
    annotations.power_nets.remove("0");

    // Unify regulator decomposition and cascade currents
    struct RegInfo {
        base_name: String,
        vout_current: f64,
        vout_node: String,
        vin_node: String,
    }
    let mut regulators: Vec<RegInfo> = Vec::new();

    for (edge_idx, current) in &dc_result.branch_currents {
        if let Some(branch) = circuit.graph.edge_weight(*edge_idx) {
            let is_vout = branch
                .metadata
                .get(bhdl_spice::META_DECOMPOSITION_ROLE)
                .map(|r| r.as_str())
                == Some("vout");
            if is_vout {
                let base = branch
                    .metadata
                    .get(bhdl_spice::META_PARENT_INSTANCE)
                    .cloned()
                    .unwrap_or_default();
                if let Some((src, _tgt)) = circuit.branch_nodes(*edge_idx) {
                    let vout_node = circuit.get_node_name(src).unwrap_or("").to_string();
                    let vin_node = dc_result
                        .branch_currents
                        .keys()
                        .filter_map(|eidx| {
                            let b = circuit.graph.edge_weight(*eidx)?;
                            if b.metadata
                                .get(bhdl_spice::META_PARENT_INSTANCE)
                                .map(|s| s.as_str())
                                == Some(&base)
                                && b.metadata
                                    .get(bhdl_spice::META_DECOMPOSITION_ROLE)
                                    .map(|s| s.as_str())
                                    == Some("dropout")
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
                    });
                }
            }
        }
    }

    let mut reg_currents: HashMap<String, f64> = regulators
        .iter()
        .map(|r| (r.base_name.clone(), r.vout_current))
        .collect();

    for _ in 0..regulators.len() {
        let snapshot = reg_currents.clone();
        for reg in &regulators {
            let downstream_sum: f64 = regulators
                .iter()
                .filter(|d| d.vin_node == reg.vout_node && d.base_name != reg.base_name)
                .map(|d| snapshot.get(&d.base_name).copied().unwrap_or(0.0))
                .sum();
            reg_currents.insert(reg.base_name.clone(), reg.vout_current + downstream_sum);
        }
    }

    for reg in &regulators {
        let current = reg_currents
            .get(&reg.base_name)
            .copied()
            .unwrap_or(reg.vout_current);
        annotations
            .instance_currents
            .insert(reg.base_name.clone(), current);

        let decomposed_keys: Vec<String> = dc_result
            .branch_currents
            .keys()
            .filter_map(|eidx| {
                let b = circuit.graph.edge_weight(*eidx)?;
                if b.metadata
                    .get(bhdl_spice::META_PARENT_INSTANCE)
                    .map(|s| s.as_str())
                    == Some(&reg.base_name)
                {
                    Some(b.name.clone())
                } else {
                    None
                }
            })
            .collect();

        let total_power: f64 = decomposed_keys
            .iter()
            .filter_map(|key| annotations.instance_power.get(key).copied())
            .sum();
        annotations
            .instance_power
            .insert(reg.base_name.clone(), total_power);

        for key in &decomposed_keys {
            annotations.instance_currents.remove(key);
            annotations.instance_power.remove(key);
        }
    }

    annotations
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("\n=== PnR Semantic Preprocessor Test ===\n");

    let test_file = std::env::args().nth(1).unwrap_or_else(|| {
        "tests/circuits/simple/complex_power_tree.bhdl".to_string()
    });

    let source = fs::read_to_string(&test_file)
        .with_context(|| format!("Failed to read {}", test_file))?;

    // 1. Parse + Analyze
    println!("Step 1: Parse + Analyze");
    let parsed = parse(&source);
    let parse_errors = parsed.errors();
    if !parse_errors.is_empty() {
        for e in parse_errors {
            eprintln!("  Parse error: {}", e.message);
        }
        anyhow::bail!("Parse failed with {} error(s)", parse_errors.len());
    }
    let source_file = SourceFile::cast(parsed.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    let analysis = analyze(&source_file);
    println!("  OK: {} diagnostics", analysis.diagnostics.len());

    // 2. Synthesize
    println!("Step 2: Synthesize");
    let mut generator = NetlistGenerator::new();
    let mut netlist = generator
        .generate_from_ast_and_analysis(&source_file, &analysis)
        .await?;
    println!("  OK: {} instances, {} nets", netlist.instances.len(), netlist.nets.len());

    // 3. Expansion
    println!("Step 3: Entity Expansion");
    let _recipe_results = bhdl_synthesizer::expansion_interpreter::expand_entity_instances(
        &mut netlist,
        &analysis.expansion_recipes,
    );
    let _vpin_results =
        bhdl_synthesizer::virtual_pin_expander::expand_virtual_pins(&mut netlist);
    println!("  OK: {} instances post-expansion", netlist.instances.len());

    // 4. GLACIER DC
    println!("Step 4: GLACIER DC Simulation");
    let mut converter = NetlistToSpiceConverter::new();
    let circuit = converter.convert(&netlist).context("SPICE conversion failed")?;
    let circuit_ref = circuit.clone();
    let solver = bhdl_spice::GlacierDcSolver::new();
    let dc_result = solver.solve(circuit).context("GLACIER DC failed")?;
    let annotations = build_simulation_annotations(&dc_result, &circuit_ref);
    println!("  OK: converged in {} iterations", dc_result.iterations);

    // 5. Physical selection
    println!("Step 5: Physical Selection");
    let phys_results =
        bhdl_synthesizer::glacier_physical_selection::apply_glacier_physical_selection(
            &mut netlist,
            &annotations.instance_currents,
            &annotations.instance_power,
            &annotations.net_voltages,
        );
    println!("  OK: {} components got physical params", phys_results.len());

    // 6. Build PnR Board
    println!("Step 6: build_board()");
    let board = semantic::build_board(
        &netlist,
        Some(&annotations),
        SemanticConfig::default(),
    )?;

    println!("\n=== Board Summary ===");
    println!("  Components: {}", board.components.len());
    println!("  Nets:       {}", board.nets.len());
    println!("  Groups:     {}", board.groups.len());
    println!("  Layers:     {}", board.layer_stack.layers.len());
    println!("  Outline:    {:?}", board.config.outline);

    // Validate
    let mut errors = 0;

    for comp in &board.components {
        if comp.width_mm <= 0.0 || comp.height_mm <= 0.0 {
            eprintln!("  ERROR: {} has zero dimensions ({:.1}x{:.1}mm)",
                comp.name, comp.width_mm, comp.height_mm);
            errors += 1;
        }
        if comp.pins.is_empty() {
            eprintln!("  ERROR: {} has no pins", comp.name);
            errors += 1;
        }
    }

    let nets_with_pins: usize = board.nets.iter().filter(|n| n.pins.len() >= 2).count();
    let empty_nets: usize = board.nets.iter().filter(|n| n.pins.is_empty()).count();

    println!("\n=== Components ===");
    for comp in &board.components {
        let pkg_known = ipc7351::standard_package(&comp.package).is_some();
        println!("  {} ({}) {:.1}x{:.1}mm {} pins pkg={} [{}]",
            comp.refdes, comp.name,
            comp.width_mm, comp.height_mm,
            comp.pins.len(), comp.package,
            if pkg_known { "IPC" } else { "fallback" });
    }

    println!("\n=== Nets (with 2+ pins) ===");
    for net in board.nets.iter().filter(|n| n.pins.len() >= 2) {
        println!("  {} ({:?}) {} pins, trace={:.2}mm",
            net.name, net.net_class, net.pins.len(), net.required_trace_width_mm);
    }

    if !board.groups.is_empty() {
        println!("\n=== Functional Groups ===");
        for group in &board.groups {
            println!("  {} ({} members)", group.name, group.members.len());
        }
    }

    println!("\n=== Validation ===");
    println!("  Components with valid dimensions: {}/{}",
        board.components.iter().filter(|c| c.width_mm > 0.0 && c.height_mm > 0.0).count(),
        board.components.len());
    println!("  Nets with 2+ pins: {}/{}", nets_with_pins, board.nets.len());
    println!("  Empty nets (filtered): {}", empty_nets);
    println!("  Errors: {}", errors);

    if errors > 0 {
        anyhow::bail!("{} validation error(s)", errors);
    }

    println!("\nAll checks passed.");
    Ok(())
}
