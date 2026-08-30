//! Out-of-tree (proprietary) library resolution — the customer
//! scenario: a library outside the repo, declared in `bhdl.toml`,
//! with INTERNAL relative imports between its own files. Probed live
//! before these tests existed: the nested `./sibling.bhdl` resolved
//! against the BOARD's directory (not the importing file's), and the
//! failure was warn-swallowed — the board built green with an entity
//! that never loaded.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write(p: &Path, content: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// Lay out <dir>/acme-lib (manifest + two files, one importing the
/// other RELATIVELY) and <dir>/proj (bhdl.toml + board).
fn scaffold(dir: &Path, declared_version: &str, nested_import: &str) {
    write(
        &dir.join("acme-lib/manifest.toml"),
        "[library]\nname = \"acme-lib\"\nversion = \"1.0.0\"\n",
    );
    write(
        &dir.join("acme-lib/parts/acme_ic.bhdl"),
        &format!(
            r#"import {{ AcmeSub }} from "{nested_import}";
entity AcmeIC() {{
    pin VCC: power in;
    pin OUT: signal out;
    pin GND: ground;
    attribute component_class = "ic";
    attribute part_number = "ACME-IC-1";
}}
"#
        ),
    );
    write(
        &dir.join("acme-lib/parts/acme_sub.bhdl"),
        r#"entity AcmeSub() {
    pin A: signal in;
    pin GND: ground;
    attribute component_class = "ic";
    attribute part_number = "ACME-SUB-1";
}
"#,
    );
    write(
        &dir.join("proj/bhdl.toml"),
        &format!(
            "[project]\nname = \"oot-test\"\nversion = \"0.1.0\"\n\n[libraries]\nacme-lib = {{ path = \"../acme-lib\", version = \"{declared_version}\" }}\n"
        ),
    );
    write(
        &dir.join("proj/board.bhdl"),
        r#"import { AcmeIC } from "acme-lib/parts/acme_ic.bhdl";
board OotBoard {
    power VDD = 3.3V @ 100mA;
    ground GND;
    u1: AcmeIC();
    @VDD -> u1.VCC;
    u1.GND -> @GND;
}
"#,
    );
}

fn run(board: &Path) -> (String, bool) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
        .env_remove("BHDL_LIB_PATH")
        .arg(board)
        .arg("synthesize");
    let out = c.output().expect("spawn");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.success(),
    )
}

#[test]
fn library_internal_relative_imports_resolve() {
    let dir = std::env::temp_dir().join("bhdl_oot_relative");
    let _ = std::fs::remove_dir_all(&dir);
    scaffold(&dir, "1.0", "./acme_sub.bhdl");
    let (text, ok) = run(&dir.join("proj/board.bhdl"));
    assert!(ok, "out-of-tree board failed:\n{}",
        text.lines().rev().take(8).collect::<Vec<_>>().join("\n"));
    assert!(!text.contains("import loading failed"), "nested relative import failed:\n{}",
        text.lines().filter(|l| l.contains("import")).take(8).collect::<Vec<_>>().join("\n"));
    // the sibling actually loaded from INSIDE the library
    assert!(
        text.contains("acme_sub.bhdl") && text.contains("acme-lib"),
        "sibling not loaded from the library:\n{}",
        text.lines().filter(|l| l.contains("acme_sub")).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn missing_library_file_is_a_hard_error() {
    let dir = std::env::temp_dir().join("bhdl_oot_missing");
    let _ = std::fs::remove_dir_all(&dir);
    scaffold(&dir, "1.0", "./nope.bhdl");
    let (text, ok) = run(&dir.join("proj/board.bhdl"));
    assert!(!ok, "board with a missing library file built green — the silent-drop path is back");
    assert!(
        text.contains("import loading failed") && text.contains("nope.bhdl"),
        "failure does not name the import chain:\n{}",
        text.lines().rev().take(6).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn version_mismatch_is_refused() {
    let dir = std::env::temp_dir().join("bhdl_oot_version");
    let _ = std::fs::remove_dir_all(&dir);
    scaffold(&dir, "2.0", "./acme_sub.bhdl");
    let (text, ok) = run(&dir.join("proj/board.bhdl"));
    assert!(!ok, "version mismatch not refused");
    assert!(
        text.contains("version 1.0.0") && text.contains("requires 2.0"),
        "mismatch error does not name both versions:\n{}",
        text.lines().rev().take(6).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn declared_providers_export_pin_and_drift() {
    // `[providers]` in bhdl.toml — DECLARED, NOT AMBIENT: the
    // declared supply-chain provider is exported to the machinery,
    // pinned in bhdl.lock, authoritative over ambient env, and a
    // changed command is DRIFT (loud, --update-lock to accept).
    let dir = std::env::temp_dir().join("bhdl_oot_providers");
    let _ = std::fs::remove_dir_all(&dir);
    scaffold(&dir, "1.0", "./acme_sub.bhdl");
    let manifest = dir.join("proj/bhdl.toml");
    let base = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{base}\n[providers]\nsupply = \"python3 /opt/acme/supply.py --db /opt/acme/parts.sqlite\"\n"),
    )
    .unwrap();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let run_flags = |flags: &[&str], env: Option<(&str, &str)>| -> (String, bool) {
        let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
        c.current_dir(&root).env_remove("BHDL_LIB_PATH").env_remove("BHDL_SUPPLY_PROVIDER");
        if let Some((k, v)) = env {
            c.env(k, v);
        }
        for f in flags {
            c.arg(f);
        }
        c.arg(dir.join("proj/board.bhdl")).arg("synthesize");
        let out = c.output().expect("spawn");
        (
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
            out.status.success(),
        )
    };

    // declared → exported (verbose print) + locked
    let (text, ok) = run_flags(&["--verbose", "--update-lock"], None);
    assert!(ok, "provider board failed:\n{}", text.lines().rev().take(6).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("provider (supply): python3 /opt/acme/supply.py"), "declared provider not exported:\n{}",
        text.lines().filter(|l| l.contains("provider")).collect::<Vec<_>>().join("\n"));
    let lock = std::fs::read_to_string(dir.join("proj/bhdl.lock")).unwrap();
    assert!(lock.contains("[[provider]]") && lock.contains("role = \"supply\""), "provider not pinned:\n{lock}");

    // declared beats ambient, with a note
    let (text, _) = run_flags(&[], Some(("BHDL_SUPPLY_PROVIDER", "something-else")));
    assert!(text.contains("overrides ambient $BHDL_SUPPLY_PROVIDER"), "ambient override not reported");

    // changed command = DRIFT, refused loudly
    let drifted = std::fs::read_to_string(&manifest).unwrap().replace("parts.sqlite", "OTHER.sqlite");
    std::fs::write(&manifest, drifted).unwrap();
    let (text, ok) = run_flags(&[], None);
    assert!(!ok, "provider drift not refused");
    assert!(text.contains("provider `supply` changed"), "drift does not name the provider:\n{}",
        text.lines().rev().take(6).collect::<Vec<_>>().join("\n"));
}
