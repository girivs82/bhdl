//! End-to-end demo of the Phase 4e catalog-scan driver.
//!
//! Builds a tiny "design" by hand — a handful of InstanceClass
//! values that resemble what a small board's worth of resistors,
//! caps, and ICs would produce after monomorphization — runs the
//! catalog scan against the full Phase-3 catalog seed, and prints
//! the resulting JSON candidate bundle.
//!
//! Run with:
//!   cargo run -q -p bhdl-analyzer --bin test_catalog_scan_demo

use bhdl_analyzer::catalog_scan::{bundle_to_json, run_catalog_scan, InstanceClass};
use bhdl_analyzer::part_family::ClassInstance;
use bhdl_ast::SourceFile;
use bhdl_common::ConstValue;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use std::fs;
use std::path::Path;

fn load_catalog(root: &Path) -> Vec<(SourceFile, String)> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    for path in files {
        let content = fs::read_to_string(&path).expect("read");
        let pr = parse(&content);
        if !pr.errors().is_empty() {
            eprintln!("skip (parse errors): {}", path.display());
            continue;
        }
        let sf = SourceFile::cast(pr.syntax()).expect("source file");
        out.push((sf, path.display().to_string()));
    }
    out
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("bhdl") {
                out.push(p);
            }
        }
    }
}

fn main() {
    let catalog = load_catalog(Path::new("bhdl-stdlib/parts"));
    eprintln!("loaded {} catalog file(s)", catalog.len());

    // Hand-built "design": resembles a small mixed-signal board.
    //   - 4 × 10kΩ 1% 0603 (e.g. pull-ups)
    //   - 2 × 4.7kΩ 1% 0603 (e.g. I²C pull-ups)
    //   - 3 × 100nF X7R 0603 (decoupling)
    //   - 1 × 22µF 25V X7R 0603 (bulk)  ← won't match Murata GRM188R71H (V≠50V)
    //   - 1 × AP2112K-3.3 LDO
    //   - 1 × LM317 LDO
    let r_10k = ClassInstance {
        entity: "Resistor".to_string(),
        generics: vec![
            ConstValue::Resistance(10_000.0),
            ConstValue::String("1%".to_string()),
            ConstValue::String("0603".to_string()),
        ],
    };
    let r_4k7 = ClassInstance {
        entity: "Resistor".to_string(),
        generics: vec![
            ConstValue::Resistance(4_700.0),
            ConstValue::String("1%".to_string()),
            ConstValue::String("0603".to_string()),
        ],
    };
    let c_100n = ClassInstance {
        entity: "Capacitor".to_string(),
        generics: vec![
            ConstValue::Capacitance(100e-9),
            ConstValue::Voltage(50.0),
            ConstValue::String("X7R".to_string()),
            ConstValue::String("0603".to_string()),
        ],
    };
    let c_22u = ClassInstance {
        entity: "Capacitor".to_string(),
        generics: vec![
            ConstValue::Capacitance(22e-6),
            ConstValue::Voltage(25.0),
            ConstValue::String("X7R".to_string()),
            ConstValue::String("0603".to_string()),
        ],
    };
    let ldo_3v3 = ClassInstance {
        entity: "AP2112K".to_string(),
        generics: vec![ConstValue::Voltage(3.3)],
    };
    let ldo_lm317 = ClassInstance {
        entity: "LM317".to_string(),
        generics: vec![],
    };

    let instances = vec![
        InstanceClass { refdes: "R1".to_string(),  class: r_10k.clone() },
        InstanceClass { refdes: "R2".to_string(),  class: r_10k.clone() },
        InstanceClass { refdes: "R3".to_string(),  class: r_10k.clone() },
        InstanceClass { refdes: "R4".to_string(),  class: r_10k.clone() },
        InstanceClass { refdes: "R5".to_string(),  class: r_4k7.clone() },
        InstanceClass { refdes: "R6".to_string(),  class: r_4k7.clone() },
        InstanceClass { refdes: "C1".to_string(),  class: c_100n.clone() },
        InstanceClass { refdes: "C2".to_string(),  class: c_100n.clone() },
        InstanceClass { refdes: "C3".to_string(),  class: c_100n.clone() },
        InstanceClass { refdes: "C4".to_string(),  class: c_22u.clone() },
        InstanceClass { refdes: "U1".to_string(),  class: ldo_3v3.clone() },
        InstanceClass { refdes: "U2".to_string(),  class: ldo_lm317.clone() },
    ];

    let bundle = run_catalog_scan("DemoBoard", &instances, &catalog);

    eprintln!("\n=== Bundle summary ===");
    eprintln!("board: {}", bundle.board);
    eprintln!("selections: {}", bundle.selections_needed.len());
    for sel in &bundle.selections_needed {
        let mpns: Vec<&str> = sel.candidates.iter().map(|c| c.mpn.as_str()).collect();
        eprintln!(
            "  {} × {} ({} candidate(s)): {}",
            sel.instance_count,
            sel.class,
            sel.candidates.len(),
            if mpns.is_empty() { "—".to_string() } else { mpns.join(", ") },
        );
    }

    eprintln!("\n=== JSON ===");
    println!("{}", bundle_to_json(&bundle));
}
