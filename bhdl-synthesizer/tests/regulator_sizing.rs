//! End-to-end: the TPS54331 buck entity sizes its REACTIVE support
//! components (inductor + input/output caps) and its FB divider from
//! electrical targets via its `design { }` block, and those computed
//! values land on the expansion-child instances.
//!
//! This is the buck analogue of the LM317 resistor-sizing proof — the
//! first stdlib entity to compute henries/farads (not just ohms) and
//! flow them through the design→expansion value-substitution path. It
//! also guards the `format_designed_value` fix: a 7µH inductor is
//! `7e-6` H in SI base units, which the old `{:.3}` formatter truncated
//! to `"0.000"`.
//!
//! Sets cwd to the workspace root so the stdlib import resolves; single
//! test per binary.

use bhdl_analyzer::analyze;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;

// 12V → 3.3V @ 2A (the TPS54331_3V3 defaults). Hand-computed targets
// (SI base units, ripple_ratio=0.3, ripple_v=30mV, ripple_v_in=150mV,
// f_sw=570kHz, r_fb_bot=10k):
//   D      = 3.3/12              = 0.275
//   ΔI_L   = 0.3*2               = 0.6 A
//   L_out  = (12-3.3)*0.275 / (570e3*0.6)        ≈ 6.996e-6 H  (~7µH)
//   C_out  = 0.6 / (8*570e3*0.03)                ≈ 4.386e-6 F  (~4.4µF)
//   C_in   = 2*0.275*0.725 / (570e3*0.15)        ≈ 4.664e-6 F  (~4.7µF)
//   R_top  = 10k*(3.3-0.8)/0.8                    = 31250 Ω
const BOARD: &str = r#"
import { Ind } from "bhdl-stdlib/passive/inductor.bhdl";
import { Diode } from "bhdl-stdlib/passive/diode.bhdl";
import { TPS54331_3V3 } from "bhdl-stdlib/power/tps54331.bhdl";

board BuckSizing {
    power VIN = 12V @ 3A;
    power V3 = 3.3V @ 2A;
    ground GND;

    buck: TPS54331_3V3();
    VIN -> buck.VIN;
    buck.VOUT -> V3;
    buck.GND -> GND;
    buck.EN -> VIN;
}
"#;

#[tokio::test]
async fn tps54331_sizes_reactive_components() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    std::env::set_current_dir(&ws).expect("cwd → workspace root");

    let pr = parse(BOARD);
    assert!(pr.errors().is_empty(), "parse errors: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);

    let mut gen = NetlistGenerator::new();
    let netlist = gen
        .generate_from_ast_and_analysis(&sf, &analysis)
        .await
        .expect("synthesis");

    // The expansion-child `value` attribute is a bare SI-base float
    // string (e.g. "0.000006996" for 7µH, "31250.000" for the divider).
    let value_of = |needle: &str| -> f64 {
        let (_, inst) = netlist
            .instances
            .iter()
            .find(|(_, i)| i.name.contains(needle))
            .unwrap_or_else(|| panic!("expansion child `{needle}` not materialised"));
        let raw = inst
            .attributes
            .get("value")
            .unwrap_or_else(|| panic!("`{needle}` has no value attribute"));
        raw.trim()
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("`{needle}` value `{raw}` is not a bare float"))
    };

    let l = value_of("L_out");
    assert!(
        (l - 6.996e-6).abs() < 0.3e-6,
        "L_out should size to ~7µH from ripple targets, got {l} H"
    );

    let c_out = value_of("C_out");
    assert!(
        (c_out - 4.386e-6).abs() < 0.5e-6,
        "C_out should size to ~4.4µF, got {c_out} F"
    );

    let c_in = value_of("C_in");
    assert!(
        (c_in - 4.664e-6).abs() < 0.5e-6,
        "C_in should size to ~4.7µF, got {c_in} F"
    );

    let r_top = value_of("R_top");
    assert!(
        (r_top - 31250.0).abs() < 1.0,
        "R_top should size to 31.25kΩ for 3.3V out, got {r_top} Ω"
    );

    // Guard the formatter fix: a reactive value must NOT have truncated
    // to zero under the old `{:.3}`.
    assert!(l > 0.0 && c_out > 0.0 && c_in > 0.0, "reactive values must be non-zero");
}
