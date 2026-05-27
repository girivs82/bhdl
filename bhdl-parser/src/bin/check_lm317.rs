use bhdl_parser::parse;
use std::fs;
fn main() {
    let c = fs::read_to_string("bhdl-stdlib/power/lm317.bhdl").unwrap();
    let r = parse(&c);
    let n = r.errors().len();
    if n == 0 { println!("✓ parses cleanly"); }
    else {
        println!("✗ {} parse errors:", n);
        for e in r.errors() { println!("   {:?}", e); }
    }
}
