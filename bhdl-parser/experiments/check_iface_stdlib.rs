//! Walks `bhdl-stdlib/**/*.bhdl` and confirms every file parses
//! with zero errors. Added with the v0.1 interface work to keep
//! the canonical interface declarations parser-validated; expanded
//! at v0.7c to cover the whole stdlib (after the `~Interface` →
//! `:perspective` migration). Accepts an optional CLI argument
//! pointing at a different root, defaulting to `bhdl-stdlib/`.

use bhdl_parser::parse;
use std::fs;
use std::path::Path;

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { collect(&p, out); }
            else if p.extension().and_then(|s| s.to_str()) == Some("bhdl") { out.push(p); }
        }
    }
}

fn main() {
    let arg = std::env::args().nth(1);
    let root_path = arg.unwrap_or_else(|| "bhdl-stdlib".to_string());
    let root = Path::new(&root_path);
    if !root.exists() {
        eprintln!("root not found: {}", root.display());
        std::process::exit(2);
    }
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    if files.is_empty() {
        eprintln!("no interface files found");
        std::process::exit(2);
    }

    let mut bad = 0;
    for path in &files {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => { eprintln!("✗ {} — read error: {}", path.display(), e); bad += 1; continue; }
        };
        let r = parse(&content);
        if r.errors().is_empty() {
            println!("✓ {}", path.display());
        } else {
            println!("✗ {} — {} parse error(s)", path.display(), r.errors().len());
            for err in r.errors().iter().take(5) { println!("    {:?}", err); }
            bad += 1;
        }
    }
    println!("\n{} files, {} failed", files.len(), bad);
    if bad > 0 { std::process::exit(1); }
}
