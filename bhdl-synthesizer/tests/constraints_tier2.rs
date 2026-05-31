//! Tier-2 interface constraints (task #96): multi-value storage, override
//! precedence, and the provenance sidecar map.
//!
//! When more than one `constraints { }` statement targets the same
//! `(pin, prop)`, the synthesizer no longer silently overwrites. It keeps
//! every contributor, picks a winner by override precedence (an explicit
//! pin target beats a wildcard; same-tier ties go to the last writer), and
//! emits the full contributor list once per module under
//! `INTERFACE_CONSTRAINT_PROVENANCE_ATTR` as a JSON
//! `ConstraintProvenanceMap`. The P&R session consumes that map for
//! traceable diagnostics + same-tier contradiction detection.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_common::constraint_provenance::{
    ConstraintProvenance, ConstraintProvenanceMap, ConstraintTier,
    INTERFACE_CONSTRAINT_PROVENANCE_ATTR,
};
use bhdl_parser::parse;

// `*` sets a broad 40ohm; `D0` overrides to 50ohm (specific > wildcard);
// `D2` is set twice with explicit-but-different values (a same-tier
// contradiction). `D1` is only touched by the wildcard.
const SOURCE: &str = r#"
interface Bus {
    signal D0: inout;
    signal D1: inout;
    signal D2: inout;
    constraints {
        *:  single_ended 40ohm;
        D0: single_ended 50ohm;
        D2: single_ended 40ohm;
        D2: single_ended 60ohm;
    }
}

entity Dev {
    interface Bus bus;
}

board TestBoard {
    power VCC = 1.2V @ 1A;
    ground GND;

    d: Dev();
}
"#;

#[test]
fn tier2_multivalue_precedence_and_provenance() {
    let pr = parse(SOURCE);
    assert!(pr.errors().is_empty(), "parse errors: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = bhdl_analyzer::analyze(&sf);
    let mut netlist = bhdl_netlist::Netlist::new();
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf, &analysis, &mut netlist, None,
    )
    .expect("synthesis succeeded");

    let module_id = netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == "d")
        .map(|(_, i)| i.definition)
        .expect("d instance");
    let module = netlist.modules.get(module_id).expect("d module");

    // --- Primary attributes carry the override winners (back-compat) ---
    let attr = |k: &str| module.attributes.get(k).cloned();
    assert_eq!(
        attr("intf_const__bus.D0__single_ended").as_deref(),
        Some("50ohm"),
        "specific D0 override must beat the wildcard 40ohm"
    );
    assert_eq!(
        attr("intf_const__bus.D1__single_ended").as_deref(),
        Some("40ohm"),
        "D1 only matched the wildcard"
    );
    assert_eq!(
        attr("intf_const__bus.D2__single_ended").as_deref(),
        Some("60ohm"),
        "same-tier tie resolves to last writer"
    );

    // --- The provenance map is present and decodes ---
    let raw = attr(INTERFACE_CONSTRAINT_PROVENANCE_ATTR)
        .expect("provenance sidecar attribute present");
    let prov: ConstraintProvenanceMap =
        serde_json::from_str(&raw).expect("provenance map decodes");

    // D0: two contributors (wildcard 40ohm Interface, specific 50ohm Specific).
    let d0 = prov
        .get("intf_const__bus.D0__single_ended")
        .expect("D0 provenance");
    assert_eq!(d0.len(), 2, "D0 has two contributors");
    let winner = ConstraintProvenance::winner(d0).unwrap();
    assert_eq!(winner.value, "50ohm");
    assert_eq!(winner.tier, ConstraintTier::Specific);
    assert_eq!(winner.scope, "Bus", "scope is the declaring interface type");
    assert!(winner.line.is_some(), "source line recovered for inline source");
    assert!(
        !ConstraintProvenance::has_same_tier_conflict(d0),
        "specific-over-wildcard is an override, not a conflict"
    );

    // D2: a genuine same-tier contradiction (two explicit, differing values).
    let d2 = prov
        .get("intf_const__bus.D2__single_ended")
        .expect("D2 provenance");
    assert!(
        ConstraintProvenance::has_same_tier_conflict(d2),
        "D2 has two explicit (Specific) statements with different values"
    );

    // D1: single contributor, still recorded with provenance.
    let d1 = prov
        .get("intf_const__bus.D1__single_ended")
        .expect("D1 provenance");
    assert_eq!(d1.len(), 1);
    assert_eq!(d1[0].tier, ConstraintTier::Interface);
}
