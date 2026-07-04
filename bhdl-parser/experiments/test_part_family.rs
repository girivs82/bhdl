//! Phase 2 verification: parse a `part_family` declaration.
//!
//! Walks every `.bhdl` file under `bhdl-stdlib/parts/` and confirms
//! the parser accepts it without error. Phase 2 ships with just one
//! sample (TI_LM317T); future phases will grow the catalog.

use bhdl_parser::parse;
use std::fs;
use std::path::Path;

fn collect_bhdl_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_bhdl_files(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("bhdl") {
                out.push(p);
            }
        }
    }
}

fn main() {
    let root = Path::new("bhdl-stdlib/parts");
    if !root.exists() {
        eprintln!("part catalog directory not found: {}", root.display());
        std::process::exit(2);
    }

    let mut files = Vec::new();
    collect_bhdl_files(root, &mut files);
    files.sort();

    if files.is_empty() {
        eprintln!("no .bhdl files found under {}", root.display());
        std::process::exit(2);
    }

    let mut ok = 0;
    let mut bad = 0;
    for path in &files {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("✗ {} — cannot read: {}", path.display(), e);
                bad += 1;
                continue;
            }
        };
        let result = parse(&content);
        if result.errors().is_empty() {
            println!("✓ {}", path.display());
            ok += 1;
        } else {
            println!("✗ {} — {} parse error(s)", path.display(), result.errors().len());
            for err in result.errors().iter().take(5) {
                println!("    {:?}", err);
            }
            bad += 1;
        }
    }

    println!("\n{} passed, {} failed (of {})", ok, bad, files.len());
    if bad > 0 {
        std::process::exit(1);
    }
}
