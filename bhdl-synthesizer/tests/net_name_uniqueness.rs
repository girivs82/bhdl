//! Every net name in a synthesized netlist must be UNIQUE: the spice
//! converter coalesces same-named nets into one node, so a duplicate is
//! invisible to simulation — but any consumer keying by NetId (fault
//! campaign, PnR, exports) silently misses the copy. Regression for the
//! chain-opening-pin duplication (two chains starting at the same pin
//! each minted an `auto_…` net with the same name).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;

async fn assert_unique(rel: &str) {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join(rel)).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "{rel} parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut names: Vec<String> = netlist.nets.iter().filter_map(|(_, n)| n.name.clone()).collect();
    let total = names.len();
    names.sort();
    let mut dups: Vec<&String> = names.windows(2).filter(|w| w[0] == w[1]).map(|w| &w[0]).collect();
    dups.dedup();
    assert!(dups.is_empty(), "{rel}: {} nets, duplicate names: {:?}", total, dups);
}

#[tokio::test]
async fn fault_campaign_fixture_net_names_are_unique() {
    assert_unique("tests/circuits/realistic/test_safety_fault_campaign.bhdl").await;
}

#[tokio::test]
async fn fit_divider_fixture_net_names_are_unique() {
    assert_unique("tests/circuits/realistic/test_safety_fit_divider.bhdl").await;
}

#[tokio::test]
async fn supervised_reg_fixture_net_names_are_unique() {
    assert_unique("tests/circuits/realistic/test_safety_supervised_reg.bhdl").await;
}
