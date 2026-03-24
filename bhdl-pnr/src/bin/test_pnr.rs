//! End-to-end PnR test: Full BHDL pipeline → place_and_route().
//!
//! Run: cargo run -p bhdl-pnr --bin test_pnr [circuit.bhdl]

use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_spice::NetlistToSpiceConverter;

use bhdl_pnr::semantic::{self, SemanticConfig};
use bhdl_pnr::types::PnrConfig;

// Reuse the simulation annotation builder from test_semantic
const POWER_THRESHOLD_AMPS: f64 = 1e-3;

fn build_simulation_annotations(
    dc_result: &bhdl_spice::DcAnalysisResult,
    circuit: &bhdl_spice::Circuit,
) -> bhdl_schematic::SimulationAnnotations {
    let mut ann = bhdl_schematic::SimulationAnnotations::default();
    for (idx, v) in &dc_result.node_voltages {
        if let Some(name) = circuit.get_node_name(*idx) {
            ann.net_voltages.insert(name.to_string(), *v);
        }
    }
    for (eidx, current) in &dc_result.branch_currents {
        if let Some(branch) = circuit.graph.edge_weight(*eidx) {
            ann.instance_currents.insert(branch.name.clone(), *current);
            if let Some((src, tgt)) = circuit.branch_nodes(*eidx) {
                let vs = dc_result.node_voltages.get(&src).unwrap_or(&0.0);
                let vt = dc_result.node_voltages.get(&tgt).unwrap_or(&0.0);
                ann.instance_power.insert(branch.name.clone(), (vs - vt).abs() * current.abs());
            }
        }
    }
    for (eidx, current) in &dc_result.branch_currents {
        if current.abs() >= POWER_THRESHOLD_AMPS {
            if let Some((src, tgt)) = circuit.branch_nodes(*eidx) {
                if let Some(n) = circuit.get_node_name(src) { ann.power_nets.insert(n.to_string()); }
                if let Some(n) = circuit.get_node_name(tgt) { ann.power_nets.insert(n.to_string()); }
            }
        }
    }
    ann.power_nets.remove("GND");
    ann.power_nets.remove("0");

    // Unify regulator decomposition
    struct RegInfo { base: String, vout_i: f64, vout_node: String, vin_node: String }
    let mut regs: Vec<RegInfo> = Vec::new();
    for (eidx, current) in &dc_result.branch_currents {
        if let Some(b) = circuit.graph.edge_weight(*eidx) {
            if b.metadata.get(bhdl_spice::META_DECOMPOSITION_ROLE).map(|r| r.as_str()) == Some("vout") {
                let base = b.metadata.get(bhdl_spice::META_PARENT_INSTANCE).cloned().unwrap_or_default();
                if let Some((src, _)) = circuit.branch_nodes(*eidx) {
                    let vout_node = circuit.get_node_name(src).unwrap_or("").to_string();
                    let vin_node = dc_result.branch_currents.keys().filter_map(|ei| {
                        let bi = circuit.graph.edge_weight(*ei)?;
                        if bi.metadata.get(bhdl_spice::META_PARENT_INSTANCE).map(|s| s.as_str()) == Some(&base)
                            && bi.metadata.get(bhdl_spice::META_DECOMPOSITION_ROLE).map(|s| s.as_str()) == Some("dropout") {
                            let (s, _) = circuit.branch_nodes(*ei)?;
                            circuit.get_node_name(s).map(|n| n.to_string())
                        } else { None }
                    }).next().unwrap_or_default();
                    regs.push(RegInfo { base, vout_i: current.abs(), vout_node, vin_node });
                }
            }
        }
    }
    let mut rc: HashMap<String, f64> = regs.iter().map(|r| (r.base.clone(), r.vout_i)).collect();
    for _ in 0..regs.len() {
        let snap = rc.clone();
        for reg in &regs {
            let ds: f64 = regs.iter()
                .filter(|d| d.vin_node == reg.vout_node && d.base != reg.base)
                .map(|d| snap.get(&d.base).copied().unwrap_or(0.0)).sum();
            rc.insert(reg.base.clone(), reg.vout_i + ds);
        }
    }
    for reg in &regs {
        let cur = rc.get(&reg.base).copied().unwrap_or(reg.vout_i);
        ann.instance_currents.insert(reg.base.clone(), cur);
        let decomposed: Vec<String> = dc_result.branch_currents.keys().filter_map(|ei| {
            let b = circuit.graph.edge_weight(*ei)?;
            if b.metadata.get(bhdl_spice::META_PARENT_INSTANCE).map(|s| s.as_str()) == Some(&reg.base) {
                Some(b.name.clone())
            } else { None }
        }).collect();
        let tp: f64 = decomposed.iter().filter_map(|k| ann.instance_power.get(k).copied()).sum();
        ann.instance_power.insert(reg.base.clone(), tp);
        for k in &decomposed { ann.instance_currents.remove(k); ann.instance_power.remove(k); }
    }
    ann
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    println!("\n=== End-to-End PnR Test ===\n");

    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/simple/complex_power_tree.bhdl".to_string());
    let source = fs::read_to_string(&test_file).with_context(|| format!("Failed to read {}", test_file))?;

    // 1. Parse + Analyze
    print!("Parse + Analyze... ");
    let parsed = parse(&source);
    if !parsed.errors().is_empty() { anyhow::bail!("Parse failed"); }
    let sf = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&sf);
    println!("OK");

    // 2. Synthesize
    print!("Synthesize... ");
    let mut gen = NetlistGenerator::new();
    let mut netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;
    println!("OK ({} instances)", netlist.instances.len());

    // 3. Expand
    print!("Expand... ");
    bhdl_synthesizer::expansion_interpreter::expand_entity_instances(&mut netlist, &analysis.expansion_recipes);
    bhdl_synthesizer::virtual_pin_expander::expand_virtual_pins(&mut netlist);
    println!("OK ({} instances)", netlist.instances.len());

    // 4. GLACIER
    print!("GLACIER DC... ");
    let mut conv = NetlistToSpiceConverter::new();
    let circuit = conv.convert(&netlist).context("SPICE conversion")?;
    let cref = circuit.clone();
    let solver = bhdl_spice::GlacierDcSolver::new();
    let dc = solver.solve(circuit).context("GLACIER DC failed")?;
    let ann = build_simulation_annotations(&dc, &cref);
    println!("OK ({} iters)", dc.iterations);

    // 5. Physical selection
    print!("Physical selection... ");
    let phys = bhdl_synthesizer::glacier_physical_selection::apply_glacier_physical_selection(
        &mut netlist, &ann.instance_currents, &ann.instance_power, &ann.net_voltages);
    println!("OK ({} components)", phys.len());

    // 6. Build board
    print!("build_board... ");
    let board = semantic::build_board(&netlist, Some(&ann), SemanticConfig::default())?;
    println!("OK ({} comps, {} nets, {} groups)", board.components.len(), board.nets.len(), board.groups.len());

    // 7. Place & Route
    println!("place_and_route...");
    // Use proposal-recommended iterations (800 default with tiered routing
    // at 100/400 boundaries). Run 3 trials to find best placement.
    let config = PnrConfig {
        max_iterations: 600, // enough for coarse+fine routing phases
        ..PnrConfig::default()
    };
    let result = bhdl_pnr::place_and_route_best_of(board, config, 3)?;

    // 8. Results
    println!("\n=== PnR Results ===");
    println!("  HPWL:          {:.1} mm", result.metrics.hpwl_mm);
    println!("  Routed length: {:.1} mm", result.metrics.total_routed_length_mm);
    println!("  Via count:     {}", result.metrics.via_count);
    println!("  Routability:   {:.1}%", result.metrics.routability_pct);
    println!("  DRC violations: {}", result.drc_violations.len());

    // Verify components are within board
    let bw = result.board.config.outline.width();
    let bh = result.board.config.outline.height();
    let mut out_of_bounds = 0;
    for comp in &result.board.components {
        if comp.x < 0.0 || comp.x > bw || comp.y < 0.0 || comp.y > bh {
            out_of_bounds += 1;
        }
    }
    println!("  Out of bounds: {}/{}", out_of_bounds, result.board.components.len());

    // Check for overlapping components (post-legalization)
    let mut overlaps = 0;
    let comps = &result.board.components;
    for i in 0..comps.len() {
        for j in (i + 1)..comps.len() {
            let a = &comps[i];
            let b = &comps[j];
            let dx = (a.x - b.x).abs();
            let dy = (a.y - b.y).abs();
            let min_dx = (a.width_mm + b.width_mm) / 2.0;
            let min_dy = (a.height_mm + b.height_mm) / 2.0;
            if dx < min_dx * 0.9 && dy < min_dy * 0.9 {
                overlaps += 1;
                println!("    OVERLAP: {} ({:.1}x{:.1}) at ({:.1},{:.1}) vs {} ({:.1}x{:.1}) at ({:.1},{:.1})",
                    a.refdes, a.width_mm, a.height_mm, a.x, a.y,
                    b.refdes, b.width_mm, b.height_mm, b.x, b.y);
            }
        }
    }
    println!("  Overlapping pairs: {}", overlaps);

    // Print component positions
    println!("\n=== Component Positions ===");
    for comp in &result.board.components {
        println!("  {} ({}) at ({:.1}, {:.1}) θ={:.0}°",
            comp.refdes, comp.name, comp.x, comp.y, comp.theta.to_degrees());
    }

    println!("\nDone.");
    Ok(())
}
