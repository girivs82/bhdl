//! Integration test for the frozen structural netlist (`bhdl freeze`).
//!
//! Synthesizes the annotated ATmega decoupling board and freezes it,
//! asserting the as-fabbed record is: complete (mcu + its expansion
//! children), flat (nets carry (refdes, pin) endpoints), pure
//! (synthesis-internal / intent attributes stripped), stable (sorted),
//! and self-describing (provenance), and that it round-trips through
//! JSON unchanged.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::freeze::{freeze_netlist, FrozenNetlist, Provenance, FROZEN_SCHEMA_VERSION};
use bhdl_synthesizer::NetlistGenerator;

async fn frozen_board(path: &str) -> FrozenNetlist {
    // `cargo test` runs the integration binary with cwd = the crate
    // dir, but the fixture imports `bhdl-stdlib/…` via the legacy
    // literal-path loader (relative to cwd). Pin cwd to the workspace
    // root so both the fixture read and its stdlib imports resolve.
    // (Single test per binary → no parallel-cwd race.)
    let ws_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    std::env::set_current_dir(ws_root).expect("set cwd to workspace root");

    let src = std::fs::read_to_string(path).expect("read fixture");
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen
        .generate_from_ast_and_analysis(&sf, &analysis)
        .await
        .expect("synthesize");

    freeze_netlist(
        &netlist,
        Provenance {
            tool_version: "test".into(),
            source: path.into(),
            generated_at: "2026-01-01T00:00:00Z".into(),
            libraries: vec![],
        },
    )
}

#[tokio::test]
async fn freezes_atmega_decoupling_as_fabbed() {
    let f = frozen_board("tests/circuits/realistic/atmega328p_i2c_used.bhdl").await;

    assert_eq!(f.schema_version, FROZEN_SCHEMA_VERSION);
    assert_eq!(f.provenance.tool_version, "test");

    // The MCU plus its always-on decoupling caps materialised as
    // concrete components with resolved values. Handles live in `name`;
    // `refdes` is the fab designator minted by the phase-12.7 allocator
    // (e.g. "U1", "C3") — look components up by handle.
    let by_name = |n: &str| f.components.iter().find(|c| c.name == n);
    let mcu = by_name("mcu").expect("mcu present");
    assert_eq!(mcu.component_type, "ATmega328P_DIP28");
    assert_ne!(mcu.refdes, "", "mcu should carry an allocated refdes");

    for cap in ["mcu_C_vcc", "mcu_C_avcc", "mcu_C_aref"] {
        let c = by_name(cap).unwrap_or_else(|| panic!("{cap} present"));
        assert_eq!(c.component_type, "Cap");
        assert!(c.value.is_some(), "{cap} should carry a resolved value");
    }

    // Components are sorted by refdes (stable diffs).
    let refs: Vec<&String> = f.components.iter().map(|c| &c.refdes).collect();
    let mut sorted = refs.clone();
    sorted.sort();
    assert_eq!(refs, sorted, "components must be sorted by refdes");

    // Pure: no synthesis-internal / intent attributes leak into the
    // as-fabbed record.
    for c in &f.components {
        for k in c.attributes.keys() {
            assert!(
                !k.starts_with("intf_")
                    && !k.starts_with("vpin_")
                    && !k.starts_with("expansion_")
                    && !k.starts_with("alias__"),
                "internal attribute `{k}` leaked into frozen component {}",
                c.refdes
            );
        }
    }

    // Flat connectivity: nets carry (refdes, pin) endpoints, sorted.
    assert!(!f.nets.is_empty(), "expected nets");
    for net in &f.nets {
        assert!(!net.connections.is_empty(), "net {} has no pins", net.name);
        let mut sorted = net.connections.clone();
        sorted.sort();
        assert_eq!(net.connections, sorted, "net {} connections must be sorted", net.name);
    }

    // Self-describing + stable: round-trips through JSON unchanged.
    let json = serde_json::to_string_pretty(&f).unwrap();
    let back: FrozenNetlist = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string_pretty(&back).unwrap();
    assert_eq!(json, json2, "frozen netlist must round-trip through JSON");
}
