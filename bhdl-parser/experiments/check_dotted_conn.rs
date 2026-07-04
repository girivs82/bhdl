fn main() {
    let src = r#"
interface I { signal A: out; }
entity E { interface I i; }
board B {
    power VCC = 1V @ 1A;
    ground GND;
    a: E();
    b: E();
    a.i.A -> b.i.A;
    a.i.A <-> b.i.A;
    a.i.A <=> b.i.A;
}
"#;
    let pr = bhdl_parser::parse(src);
    for e in pr.errors().iter().take(5) {
        println!("{:?}", e);
    }
    if pr.errors().is_empty() { println!("OK"); }
}
