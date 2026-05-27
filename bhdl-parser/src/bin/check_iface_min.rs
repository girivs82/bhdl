use bhdl_parser::parse;
use std::fs;
fn main() {
    let c = fs::read_to_string("/tmp/test_iface_min.bhdl").expect("read");
    let r = parse(&c);
    println!("Errors: {}", r.errors().len());
    for e in r.errors() {
        println!("  {:?}", e);
    }
    if r.errors().is_empty() {
        println!("✓ minimal interfaces parse cleanly");
    }
}
