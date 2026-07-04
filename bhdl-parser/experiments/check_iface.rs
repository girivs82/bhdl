use bhdl_parser::parse;
use std::fs;
fn main() {
    for p in &[
        "tests/circuits/simple/test_interfaces_comprehensive.bhdl",
        "tests/circuits/edge_cases/test_interface_errors.bhdl",
    ] {
        let c = fs::read_to_string(p).unwrap_or_default();
        if c.is_empty() { println!("{}: missing", p); continue; }
        let r = parse(&c);
        println!("{}: {} parse errors", p, r.errors().len());
        for e in r.errors().iter().take(3) { println!("   {:?}", e); }
    }
}
