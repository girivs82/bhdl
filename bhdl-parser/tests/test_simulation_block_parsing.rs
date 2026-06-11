//! Parser tests for the entity-level `simulation {}` device-simulation IP
//! block (docs/spec/Vendor_Simulation_Blocks.md §2): the `stress {}` surface
//! (built now) and the reserved `model {}` surface. `simulation`/`stress`/
//! `model` are contextual keywords, distinct from the testbench `simulation`
//! config block.

use bhdl_parser::{self, SyntaxKind};

fn has_kind(code: &str, kind: SyntaxKind) -> bool {
    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    parsed.syntax().descendants().any(|n| n.kind() == kind)
}

#[test]
fn test_stress_block_parses() {
    // The TPS54302 buck stress surface from the spec: const locals + dotted
    // child stress-axis assignments (`L_out.i_peak = …`, `C_out.v_ripple = …`).
    let code = r#"
entity TPS54302(v_out: voltage = 5V, v_in: voltage = 12V, i_out: current = 2A, f_sw: frequency = 500kHz) {
    pin VIN: power in;
    pin PH: power out;

    simulation {
        stress {
            const duty = vout / vin;
            const d_il = (vin - vout) * duty / (self.f_sw * L_out.value);
            L_out.i_peak   = i_out + d_il / 2;
            C_out.v_ripple = d_il / (8 * self.f_sw * C_out.value);
            C_in.v_ripple  = i_out * duty * (1 - duty) / (self.f_sw * C_in.value);
        }
    }
}
"#;
    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    let syntax = parsed.syntax();

    assert!(
        syntax.descendants().any(|n| n.kind() == SyntaxKind::SIM_BLOCK),
        "SIM_BLOCK not found"
    );
    assert!(
        syntax.descendants().any(|n| n.kind() == SyntaxKind::STRESS_BLOCK),
        "STRESS_BLOCK not found"
    );
    // Three dotted stress-axis assignments.
    let assigns = syntax
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::STRESS_ASSIGNMENT)
        .count();
    assert_eq!(assigns, 3, "expected 3 STRESS_ASSIGNMENT nodes, got {assigns}");
}

#[test]
fn test_stress_block_with_require() {
    // `require` reuses the design-block guard grammar inside `stress {}`.
    let code = r#"
entity Buck(f_sw: frequency = 500kHz) {
    pin VIN: power in;
    simulation {
        stress {
            require self.f_sw > 0 else "switching frequency must be positive";
            L_out.i_peak = i_out;
        }
    }
}
"#;
    assert!(has_kind(code, SyntaxKind::DESIGN_REQUIRE_STMT));
    assert!(has_kind(code, SyntaxKind::STRESS_ASSIGNMENT));
}

#[test]
fn test_model_block_reserved_parses() {
    // §5 model block is reserved: it must parse (balanced braces) without error
    // even though it is not yet interpreted.
    let code = r#"
entity Buck(v_out: voltage = 5V, v_in: voltage = 12V, f_sw: frequency = 500kHz) {
    pin VIN: power in;
    simulation {
        model {
            node VOUT source = self.v_out;
            node VIN  draws  = i_out * self.v_out / (self.v_in * efficiency);
        }
        stress {
            C_out.v_ripple = 0.01;
        }
    }
}
"#;
    assert!(has_kind(code, SyntaxKind::MODEL_BLOCK));
    assert!(has_kind(code, SyntaxKind::STRESS_BLOCK));
}

#[test]
fn test_entity_without_simulation_block_unaffected() {
    // A plain entity (no simulation block) still parses cleanly — the
    // contextual `simulation` intercept must not disturb ordinary entity items.
    let code = r#"
entity Plain(r: resistance = 1kΩ) {
    pin A: signal in;
    pin B: signal out;
}
"#;
    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    assert!(
        !parsed.syntax().descendants().any(|n| n.kind() == SyntaxKind::SIM_BLOCK),
        "no SIM_BLOCK should appear for a plain entity"
    );
}
