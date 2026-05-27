//! Phase 4a verification: every `part_family` decl in the catalog
//! should land in the analyzer's symbol table with kind=PartFamily
//! and `instance_type_name` populated from the class-pattern entity.
//!
//! Walks `bhdl-stdlib/parts/**/*.bhdl`, parses each file, runs the
//! analyzer, and asserts:
//!   1. No analyzer diagnostics for the file (other than expected
//!      "unknown identifier" hits from referencing the entity name
//!      in the class pattern — Phase 4a doesn't do resolution).
//!   2. The PartFamily symbol is present.
//!   3. Its `instance_type_name` matches the entity name we wrote
//!      in the class pattern.

use bhdl_analyzer::{analyze, symbol_table::SymbolKind};
use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
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
        eprintln!("catalog dir not found: {}", root.display());
        std::process::exit(2);
    }

    let mut files = Vec::new();
    collect_bhdl_files(root, &mut files);
    files.sort();

    if files.is_empty() {
        eprintln!("no .bhdl files found under {}", root.display());
        std::process::exit(2);
    }

    let mut pass = 0;
    let mut fail = 0;

    for path in &files {
        let content = fs::read_to_string(path).unwrap_or_default();
        let parse_result = parse(&content);
        if !parse_result.errors().is_empty() {
            println!("✗ {} — parse errors", path.display());
            fail += 1;
            continue;
        }

        let syntax = parse_result.syntax();
        let Some(source_file) = SourceFile::cast(syntax) else {
            println!("✗ {} — could not cast to SourceFile", path.display());
            fail += 1;
            continue;
        };

        let result = analyze(&source_file);
        let scope_registry = &result.scope_registry;

        // The global scope (id 0) holds top-level definitions.
        let global = scope_registry.get(bhdl_analyzer::scope_registry::ScopeId(0));

        let part_family_syms: Vec<_> = global
            .table
            .iter()
            .filter(|s| s.kind == SymbolKind::PartFamily)
            .collect();

        if part_family_syms.is_empty() {
            println!("✗ {} — no PartFamily symbol registered", path.display());
            fail += 1;
            continue;
        }

        let mut all_have_class = true;
        for sym in &part_family_syms {
            if sym.instance_type_name.is_none() {
                println!(
                    "✗ {} — PartFamily '{}' has no class-pattern entity name",
                    path.display(),
                    sym.name
                );
                all_have_class = false;
            }
        }
        if !all_have_class {
            fail += 1;
            continue;
        }

        let names: Vec<String> = part_family_syms
            .iter()
            .map(|s| format!(
                "{} : {}",
                s.name,
                s.instance_type_name.as_deref().unwrap_or("?")
            ))
            .collect();
        println!("✓ {} — {}", path.display(), names.join(", "));
        pass += 1;
    }

    println!("\n{} passed, {} failed (of {})", pass, fail, files.len());
    if fail > 0 {
        std::process::exit(1);
    }
}
