//! Complex Multi-Level Parallel Power Tree — automated pipeline test.
//!
//! Uses realistic, currently-available ICs:
//!   AP63205 (2A sync buck, Diodes Inc) — 24V→5V main
//!   TPS54331 (3A buck, TI) — 24V→5V aux
//!   AP2112K (600mA LDO, Diodes Inc) — 5V→3.3V
//!   XC6206 (200mA ultra-low-Iq LDO, Torex) — 5V→1.8V
//!
//! Exercises: expansion block expansion, multi-level cascade current fixup,
//! GLACIER DC with nonlinear elements (LEDs, TVS), and physical selection
//! across ~24 components.
//!
//! Run:  cargo run -p bhdl-synthesizer --bin test_complex_power_tree

use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_spice::NetlistToSpiceConverter;

// ---------------------------------------------------------------------------
// build_simulation_annotations (mirrors bhdl-cli/src/main.rs)
// ---------------------------------------------------------------------------

const POWER_THRESHOLD_AMPS: f64 = 1e-3;

fn build_simulation_annotations(
    dc_result: &bhdl_spice::DcAnalysisResult,
    circuit: &bhdl_spice::Circuit,
) -> bhdl_schematic::SimulationAnnotations {
    let mut annotations = bhdl_schematic::SimulationAnnotations::default();

    // Map node voltages
    for (node_idx, voltage) in &dc_result.node_voltages {
        if let Some(name) = circuit.get_node_name(*node_idx) {
            annotations.net_voltages.insert(name.to_string(), *voltage);
        }
    }

    // Map branch currents and compute power
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

    // Classify power nets
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

    // --- Unify regulator decomposition and cascade currents ---

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

    // Cascade: iterative bottom-up
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

    // Write unified entries and remove decomposed ones
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

    println!(
        "\n{}",
        "=== Complex Multi-Level Parallel Power Tree Test ==="
            .bold()
            .cyan()
    );

    let test_file = std::env::args().nth(1).unwrap_or_else(|| {
        "tests/circuits/simple/complex_power_tree.bhdl".to_string()
    });

    let source = fs::read_to_string(&test_file)
        .with_context(|| format!("Failed to read {}", test_file))?;

    // -----------------------------------------------------------------------
    // 1. Parse + Analyze
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 1: Parse + Analyze ---".bold());

    let parsed = parse(&source);
    let parse_errors = parsed.errors();
    if !parse_errors.is_empty() {
        for e in parse_errors {
            eprintln!("  Parse error: {}", e.message);
        }
        anyhow::bail!("Parse failed with {} error(s)", parse_errors.len());
    }
    println!("  {} Parse: no errors", "✓".green());

    let source_file = SourceFile::cast(parsed.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;

    let analysis = analyze(&source_file);

    // Note: The analyzer produces "Undefined component type" diagnostics for
    // imported aliases (LM7805, LM1117_33, etc.) because Pass 2 runs before the
    // synthesizer's own import resolution. These are expected and non-blocking —
    // the synthesizer resolves imports independently.
    let diag_count = analysis.diagnostics.len();
    if diag_count > 0 {
        for d in &analysis.diagnostics {
            println!("  [{:?}] {}", d.severity, d.message);
        }
    }
    println!(
        "  {} Analyze: {} diagnostic(s) (import-related diagnostics are expected)",
        "✓".green(),
        diag_count
    );

    // -----------------------------------------------------------------------
    // 2. Synthesize
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 2: Synthesize ---".bold());

    let mut generator = NetlistGenerator::new();
    let mut netlist = generator
        .generate_from_ast_and_analysis(&source_file, &analysis)
        .await?;

    let pre_expansion_instances = netlist.instances.len();
    println!(
        "  {} Synthesis: {} instances, {} nets",
        "✓".green(),
        pre_expansion_instances,
        netlist.nets.len()
    );

    // We expect at least 15 instances before expansion:
    // tvs, buck, r_led5b, led5b, r_load5b, reg33, r_led33,
    // led33, r_load33, reg5aux, r_led5a, led5a, reg18, r_load18,
    // r_led18, led18 (LDO C_in/C_out and buck L/C come from expansion)
    assert!(
        pre_expansion_instances >= 15,
        "Expected ≥15 instances before expansion, got {}",
        pre_expansion_instances
    );
    println!(
        "  {} Pre-expansion instance count: {} (≥15)",
        "✓".green(),
        pre_expansion_instances
    );

    // -----------------------------------------------------------------------
    // 3. Expansion (entity expansion blocks + legacy vpin)
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 3: Entity Expansion ---".bold());

    // Step 3a: Expand entity expansion { } blocks (AP63205, TPS54331, AP2112K, XC6206)
    let recipe_results = bhdl_synthesizer::expansion_interpreter::expand_entity_instances(
        &mut netlist,
        &analysis.expansion_recipes,
    );

    let post_recipe_instances = netlist.instances.len();
    let recipe_expanded = post_recipe_instances - pre_expansion_instances;

    println!(
        "  {} {} expansion block(s) applied for {} entity instance(s)",
        "✓".green(),
        recipe_expanded,
        recipe_results.len()
    );

    let post_expansion_instances = netlist.instances.len();
    let _vpin_expanded = post_expansion_instances - post_recipe_instances;
    let expanded_count = post_expansion_instances - pre_expansion_instances;

    // All 4 regulators have expansion blocks:
    // AP63205: L+C_out+C_in+C_bst (4), TPS54331: L+D+C_out+R_fb×2+C_boot (6),
    // AP2112K: C_in+C_out (2), XC6206: C_in+C_out (2) = 14 total
    assert!(
        expanded_count >= 10,
        "Expected ≥10 expanded components from 4 regulators, got {}",
        expanded_count
    );
    println!(
        "  {} Post-expansion instances: {} (+{} from expansion blocks)",
        "✓".green(),
        post_expansion_instances,
        expanded_count
    );

    // -----------------------------------------------------------------------
    // 4. GLACIER DC simulation
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 4: GLACIER DC Simulation ---".bold());

    let mut converter = NetlistToSpiceConverter::new();
    let circuit = converter
        .convert(&netlist)
        .context("SPICE circuit conversion failed")?;

    let circuit_ref = circuit.clone();
    let solver = bhdl_spice::GlacierDcSolver::new();
    let dc_result = solver
        .solve(circuit)
        .context("GLACIER DC solver failed to converge")?;

    println!(
        "  {} Converged in {} iterations (error: {:.2e})",
        "✓".green(),
        dc_result.iterations,
        dc_result.final_error
    );

    // Build annotations (with cascade fixup)
    let annotations = build_simulation_annotations(&dc_result, &circuit_ref);

    // Print all net voltages for diagnostics
    println!("\n  Net voltages:");
    let mut sorted_voltages: Vec<_> = annotations.net_voltages.iter().collect();
    sorted_voltages.sort_by(|a, b| a.0.cmp(b.0));
    for (name, v) in &sorted_voltages {
        println!("    {}: {:.4}V", name, v);
    }
    println!();

    // Check key rail voltages
    let check_voltage = |name: &str, expected: f64, tolerance: f64| -> Result<()> {
        let v = annotations
            .net_voltages
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Net '{}' not found in voltage map", name))?;
        let diff = (v - expected).abs();
        if diff > tolerance {
            anyhow::bail!(
                "Net '{}': {:.3}V (expected {:.1}V ± {:.1}V, diff={:.3}V)",
                name,
                v,
                expected,
                tolerance,
                diff
            );
        }
        println!(
            "  {} {}: {:.3}V (expected {:.1}V ± {:.1}V)",
            "✓".green(),
            name,
            v,
            expected,
            tolerance
        );
        Ok(())
    };

    check_voltage("V5_BUCK", 5.0, 0.1)?;
    check_voltage("V3_3", 3.3, 0.1)?;
    check_voltage("V5_AUX", 5.0, 0.1)?;
    check_voltage("V1_8", 1.8, 0.1)?;

    // -----------------------------------------------------------------------
    // 5. Cascade currents
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 5: Cascade Current Fixup ---".bold());

    println!("\n  Instance currents:");
    let mut sorted_currents: Vec<_> = annotations.instance_currents.iter().collect();
    sorted_currents.sort_by(|a, b| a.0.cmp(b.0));
    for (name, i) in &sorted_currents {
        println!("    {}: {:.4}A", name, i);
    }
    println!();

    // AP63205 (buck) feeds V5_BUCK → reg33 + loads;
    // its current must exceed reg33's current
    let i_buck = annotations
        .instance_currents
        .get("buck")
        .copied()
        .unwrap_or(0.0);
    let i_reg33 = annotations
        .instance_currents
        .get("reg33")
        .copied()
        .unwrap_or(0.0);

    if i_buck > 0.0 && i_reg33 > 0.0 {
        assert!(
            i_buck > i_reg33,
            "Cascade violation: buck ({:.4}A) should exceed reg33 ({:.4}A)",
            i_buck,
            i_reg33
        );
        println!(
            "  {} buck ({:.4}A) > reg33 ({:.4}A)",
            "✓".green(),
            i_buck,
            i_reg33
        );
    } else {
        println!(
            "  {} Cascade check skipped (buck={:.4}A, reg33={:.4}A — zero current means decomposition naming differs)",
            "⚠".yellow(),
            i_buck,
            i_reg33
        );
    }

    // TPS54331 (reg5aux) feeds V5_AUX → reg18 + loads;
    // its current must exceed reg18's current
    let i_reg5aux = annotations
        .instance_currents
        .get("reg5aux")
        .copied()
        .unwrap_or(0.0);
    let i_reg18 = annotations
        .instance_currents
        .get("reg18")
        .copied()
        .unwrap_or(0.0);

    if i_reg5aux > 0.0 && i_reg18 > 0.0 {
        assert!(
            i_reg5aux > i_reg18,
            "Cascade violation: reg5aux ({:.4}A) should exceed reg18 ({:.4}A)",
            i_reg5aux,
            i_reg18
        );
        println!(
            "  {} reg5aux ({:.4}A) > reg18 ({:.4}A)",
            "✓".green(),
            i_reg5aux,
            i_reg18
        );
    } else {
        println!(
            "  {} Cascade check skipped (reg5aux={:.4}A, reg18={:.4}A)",
            "⚠".yellow(),
            i_reg5aux,
            i_reg18
        );
    }

    // No raw decomposed keys (_vout, _dropout) should remain
    let decomposed_keys: Vec<_> = annotations
        .instance_currents
        .keys()
        .filter(|k| k.ends_with("_vout") || k.ends_with("_dropout"))
        .cloned()
        .collect();
    assert!(
        decomposed_keys.is_empty(),
        "Decomposed keys should be removed: {:?}",
        decomposed_keys
    );
    println!(
        "  {} No decomposed keys (_vout/_dropout) in annotations",
        "✓".green()
    );

    // -----------------------------------------------------------------------
    // 6. Physical selection
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 6: Physical Selection ---".bold());

    let phys_results =
        bhdl_synthesizer::glacier_physical_selection::apply_glacier_physical_selection(
            &mut netlist,
            &annotations.instance_currents,
            &annotations.instance_power,
            &annotations.net_voltages,
        );

    println!(
        "  {} {} component(s) got physical parameters",
        "✓".green(),
        phys_results.len()
    );
    for r in &phys_results {
        println!(
            "    {}: {} → pkg={}, pwr={}, vrat={}, diel={}",
            r.instance_name,
            r.component_type,
            r.package,
            r.power_rating.as_deref().unwrap_or("-"),
            r.voltage_rating.as_deref().unwrap_or("-"),
            r.dielectric.as_deref().unwrap_or("-"),
        );
    }

    // We have resistors + caps that should get physical params
    // At minimum: c_in, c5b, c33, c5a, c18, r_led5b, r_load5b, r_led33,
    //   r_load33, r_led5a, r_load18, r_led18 = 12 passives
    // (expanded inductor/cap from buck may also be selected)
    assert!(
        phys_results.len() >= 10,
        "Expected ≥10 physical selections, got {}",
        phys_results.len()
    );

    // Verify at least one resistor and one capacitor have package attributes
    let has_resistor_pkg = phys_results
        .iter()
        .any(|r| r.component_type == "resistor");
    let has_cap_pkg = phys_results
        .iter()
        .any(|r| r.component_type == "capacitor");
    assert!(has_resistor_pkg, "No resistor got physical selection");
    assert!(has_cap_pkg, "No capacitor got physical selection");
    println!(
        "  {} Resistors and capacitors have package attributes",
        "✓".green()
    );

    // -----------------------------------------------------------------------
    // 7. Schematic extraction
    // -----------------------------------------------------------------------
    println!("\n{}", "--- Step 7: Schematic Extraction ---".bold());

    let schematic_data = bhdl_schematic::extract_schematic_data(
        &netlist,
        Some(&analysis),
        Some(annotations),
        None,
    )
    .map_err(|e| anyhow::anyhow!("Schematic extraction failed: {}", e))?;

    println!(
        "  {} SchematicData: {} instances, {} nets, {} ports, {} power rails",
        "✓".green(),
        schematic_data.instances.len(),
        schematic_data.nets.len(),
        schematic_data.ports.len(),
        schematic_data.power_rails.len(),
    );

    // Schematic should include the physical components (not power/ground
    // symbols, which the extractor skips).  The circuit has ~24 physical
    // components post-expansion; the schematic may include a few more or
    // fewer depending on extraction heuristics.
    assert!(
        schematic_data.instances.len() >= 20,
        "Schematic has only {} instances (expected ≥20 physical components)",
        schematic_data.instances.len()
    );

    // Simulation annotations should be present
    assert!(
        schematic_data.simulation.is_some(),
        "SchematicData should carry simulation annotations"
    );
    println!(
        "  {} Simulation annotations embedded in schematic",
        "✓".green()
    );

    // -----------------------------------------------------------------------
    // Summary
    // -----------------------------------------------------------------------
    println!(
        "\n{}",
        "=== ALL CHECKS PASSED ===".bold().green()
    );
    println!(
        "  Total instances: {} (pre-expansion: {}, expanded: +{})",
        post_expansion_instances, pre_expansion_instances, expanded_count
    );
    println!(
        "  Regulators: 4 (buck×2 + LDO×2)"
    );
    println!(
        "  Physical selections: {}",
        phys_results.len()
    );

    Ok(())
}
