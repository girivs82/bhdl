//! Regression: direct-relative import strings (`import { X } from
//! "bhdl-stdlib/…"`) must resolve when the CLI runs from a working
//! directory other than the workspace root. Historically they were
//! treated as literal cwd-relative paths, so `bhdl <abs-board> …` from
//! anywhere else failed with "Error loading import" and cascading
//! "undefined component" diagnostics (verified 2026-07-12; `-I` and
//! `$BHDL_LIB_PATH` did not cover these imports either).

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bhdl-cli has a parent workspace dir")
        .to_path_buf()
}

fn stdlib_board() -> PathBuf {
    workspace_root().join("tests/circuits/realistic/arduino_uno_r3.bhdl")
}

fn run_from_foreign_cwd(extra_args: &[&str]) -> (bool, String) {
    let board = stdlib_board();
    assert!(board.is_file(), "fixture board missing: {}", board.display());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(std::env::temp_dir())
        .env_remove("BHDL_LIB_PATH")
        .args(extra_args)
        .arg(&board)
        .arg("analyze");
    let out = cmd.output().expect("failed to spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn assert_imports_resolved(ok: bool, text: &str, label: &str) {
    assert!(
        !text.contains("Error loading import"),
        "{label}: stdlib imports failed to load from a foreign cwd:\n{}",
        text.lines()
            .filter(|l| l.contains("Error loading import"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        !text.to_lowercase().contains("undefined component"),
        "{label}: undefined components (imports not registered)"
    );
    assert!(ok, "{label}: bhdl-cli exited with failure");
}

/// Bare invocation: the stdlib is found by walking up from the input
/// file's directory (no flags, no env).
#[test]
fn elaborates_stdlib_board_from_foreign_cwd() {
    let (ok, text) = run_from_foreign_cwd(&[]);
    assert_imports_resolved(ok, &text, "no flags");
}

/// `-I <workspace-root>` must also cover direct-relative stdlib imports
/// (the exact invocation that used to fail).
#[test]
fn elaborates_stdlib_board_from_foreign_cwd_with_lib_path() {
    let root = workspace_root();
    let (ok, text) = run_from_foreign_cwd(&["-I", root.to_str().unwrap()]);
    assert_imports_resolved(ok, &text, "-I workspace root");
}
