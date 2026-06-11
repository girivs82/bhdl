//! Stage-2 tests: extracting `StressRecipe`s from entity
//! `simulation { stress { } }` blocks (Vendor_Simulation_Blocks.md §4).

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::extract_stress_recipes;
use bhdl_common::stress::StressStatement;

fn source_file(src: &str) -> SourceFile {
    let parsed = parse(src);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    SourceFile::cast(parsed.syntax()).unwrap()
}

#[test]
fn extracts_stress_statements_in_order() {
    let sf = source_file(r#"
entity TPS54302(v_out: voltage = 5V, v_in: voltage = 12V, i_out: current = 2A, f_sw: frequency = 500kHz) {
    pin VIN: power in;
    pin PH: power out;
    simulation {
        stress {
            const duty = vout / vin;
            const d_il = (vin - vout) * duty / (self.f_sw * L_out.value);
            L_out.i_peak   = i_out + d_il / 2;
            C_out.v_ripple = d_il / (8 * self.f_sw * C_out.value);
        }
    }
}
"#);

    let recipes = extract_stress_recipes(&sf);
    let r = recipes.get("TPS54302").expect("recipe for TPS54302");
    assert_eq!(r.statements.len(), 4, "two consts + two assignments");

    // Statement order is source order: const, const, assign, assign.
    match &r.statements[0] {
        StressStatement::Let { name, expr } => {
            assert_eq!(name, "duty");
            assert_eq!(expr, "vout / vin");
        }
        other => panic!("expected Let, got {other:?}"),
    }
    match &r.statements[2] {
        StressStatement::Assign { child_name, axis, expr } => {
            assert_eq!(child_name, "L_out");
            assert_eq!(axis, "i_peak");
            assert_eq!(expr, "i_out + d_il / 2");
        }
        other => panic!("expected Assign, got {other:?}"),
    }
    match &r.statements[3] {
        StressStatement::Assign { child_name, axis, .. } => {
            assert_eq!(child_name, "C_out");
            assert_eq!(axis, "v_ripple");
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}

#[test]
fn extracts_require_guard() {
    let sf = source_file(r#"
entity Buck(f_sw: frequency = 500kHz) {
    pin VIN: power in;
    simulation {
        stress {
            require self.f_sw > 0 else "f_sw must be positive";
            L_out.i_peak = i_out;
        }
    }
}
"#);
    let r = extract_stress_recipes(&sf).remove("Buck").expect("recipe");
    assert!(matches!(&r.statements[0], StressStatement::Require { message, .. } if message == "f_sw must be positive"));
}

#[test]
fn extracts_model_node_statements() {
    use bhdl_analyzer::extract_model_recipes;
    use bhdl_common::model::ModelRole;
    let sf = source_file(r#"
entity DemoBuck(v_out: voltage = 5V, v_in: voltage = 12V, efficiency: float = 0.9) {
    pin VIN: power in;
    pin VOUT: power out;
    simulation {
        model {
            node VOUT source = self.v_out;
            node VIN  draws  = i_out * self.v_out / (self.v_in * efficiency);
        }
    }
}
"#);
    let recipes = extract_model_recipes(&sf);
    let r = recipes.get("DemoBuck").expect("model recipe for DemoBuck");
    assert_eq!(r.nodes.len(), 2);
    assert_eq!(r.source_for("VOUT"), Some("self.v_out"));
    assert_eq!(r.nodes[1].role, ModelRole::Draws);
    assert_eq!(r.draws_for("VIN"), Some("i_out * self.v_out / (self.v_in * efficiency)"));
}

#[test]
fn entity_without_stress_block_is_absent() {
    let sf = source_file(r#"
entity Plain(r: resistance = 1kΩ) {
    pin A: signal in;
    pin B: signal out;
}
"#);
    assert!(extract_stress_recipes(&sf).is_empty());
}
