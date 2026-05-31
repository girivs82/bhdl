//! End-to-end test for Cargo-style library resolution: a proprietary
//! library resolved through the real `ImportLoader` + `LibraryResolver`
//! on actual fixture files. See `docs/spec/Library_Resolution.md`.
//!
//! Proves the import loader pulls an entity from a declared, namespaced
//! proprietary library — resolved via an explicit `path =`, via a `-I`
//! search root, and via `$BHDL_LIB_PATH` — and rejects the negative
//! cases (undeclared namespace, version mismatch).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_common::library::LibraryResolver;
use bhdl_parser::parse;
use bhdl_synthesizer::import_loader::ImportLoader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("bhdl_libe2e_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Write a proprietary library root with a manifest + one entity.
fn make_acme_lib(base: &Path, version: &str) -> PathBuf {
    let root = base.join("acme-stdlib");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(
        root.join("manifest.toml"),
        format!("[library]\nname = \"acme-stdlib\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("parts/widget.bhdl"),
        "entity AcmeWidget {\n    pin A: signal inout;\n    pin B: signal inout;\n}\n",
    )
    .unwrap();
    root
}

const BOARD: &str = r#"
import { AcmeWidget } from "acme-stdlib/parts/widget.bhdl";

board Demo {
    power VCC = 3.3V @ 1A;
    ground GND;
    w: AcmeWidget();
}
"#;

fn load_with_resolver(resolver: LibraryResolver) -> ImportLoader {
    let pr = parse(BOARD);
    assert!(pr.errors().is_empty(), "board parse errors: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let mut loader = ImportLoader::new(".");
    loader.set_resolver(resolver);
    loader.process_imports(&sf).expect("process_imports");
    loader
}

#[test]
fn proprietary_lib_via_explicit_path() {
    let t = tmp();
    let libs = t.join("vendor");
    std::fs::create_dir_all(&libs).unwrap();
    make_acme_lib(&libs, "2.1.0");
    let mp = t.join("bhdl.toml");
    std::fs::write(
        &mp,
        "[project]\nname=\"demo\"\n[libraries]\nacme-stdlib = { path = \"vendor/acme-stdlib\", version = \"2.1\" }\n",
    )
    .unwrap();

    let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
    let loader = load_with_resolver(r);
    assert!(
        loader.get_entity("AcmeWidget").is_some(),
        "AcmeWidget should resolve from the path-declared proprietary lib"
    );
}

#[test]
fn proprietary_lib_via_dash_i() {
    let t = tmp();
    let libs = t.join("search_root");
    std::fs::create_dir_all(&libs).unwrap();
    make_acme_lib(&libs, "2.1.5");
    let mp = t.join("bhdl.toml");
    std::fs::write(
        &mp,
        "[project]\nname=\"demo\"\n[libraries]\nacme-stdlib = \"2.1\"\n",
    )
    .unwrap();

    // No path= → resolved by name against the -I search root.
    let r = LibraryResolver::new(Some(&mp), &[libs.clone()], None, None).unwrap();
    let loader = load_with_resolver(r);
    assert!(loader.get_entity("AcmeWidget").is_some(), "should resolve via -I root");
}

#[test]
fn proprietary_lib_via_env_path() {
    let t = tmp();
    let libs = t.join("env_root");
    std::fs::create_dir_all(&libs).unwrap();
    make_acme_lib(&libs, "2.1.0");
    let mp = t.join("bhdl.toml");
    std::fs::write(
        &mp,
        "[project]\nname=\"demo\"\n[libraries]\nacme-stdlib = { version = \"2.1\" }\n",
    )
    .unwrap();

    let env = libs.to_string_lossy().to_string();
    let r = LibraryResolver::new(Some(&mp), &[], Some(&env), None).unwrap();
    let loader = load_with_resolver(r);
    assert!(loader.get_entity("AcmeWidget").is_some(), "should resolve via $BHDL_LIB_PATH");
}

#[test]
fn undeclared_namespace_is_rejected() {
    let t = tmp();
    let libs = t.join("vendor");
    std::fs::create_dir_all(&libs).unwrap();
    make_acme_lib(&libs, "2.1.0");
    // Manifest does NOT declare acme-stdlib.
    let mp = t.join("bhdl.toml");
    std::fs::write(&mp, "[project]\nname=\"demo\"\n").unwrap();

    let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
    let pr = parse(BOARD);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let mut loader = ImportLoader::new(".");
    loader.set_resolver(r);
    let err = loader.process_imports(&sf).unwrap_err().to_string();
    assert!(err.contains("not declared"), "expected 'not declared' error, got: {err}");
}

#[test]
fn version_mismatch_is_rejected() {
    let t = tmp();
    let libs = t.join("vendor");
    std::fs::create_dir_all(&libs).unwrap();
    make_acme_lib(&libs, "3.0.0"); // lib is 3.0
    let mp = t.join("bhdl.toml");
    std::fs::write(
        &mp,
        "[project]\nname=\"demo\"\n[libraries]\nacme-stdlib = { path = \"vendor/acme-stdlib\", version = \"2.1\" }\n",
    )
    .unwrap();

    let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
    let pr = parse(BOARD);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let mut loader = ImportLoader::new(".");
    loader.set_resolver(r);
    let err = loader.process_imports(&sf).unwrap_err().to_string();
    assert!(
        err.contains("version") && err.contains("2.1"),
        "expected version-mismatch error naming 2.1, got: {err}"
    );
}
