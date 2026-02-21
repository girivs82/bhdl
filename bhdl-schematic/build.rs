fn main() {
    // Tell cargo to re-run when viewer assets change (used by include_str! in html_bundle.rs)
    println!("cargo::rerun-if-changed=viewer/schematic.js");
    println!("cargo::rerun-if-changed=viewer/index.html");
}
