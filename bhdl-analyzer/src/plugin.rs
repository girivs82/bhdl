//! BOM-selection plugin invocation (Phase 5).
//!
//! Implements the user-supplied-plugin boundary spec'd in §7 of
//! `docs/spec/Parameterization_And_BOM_Resolution.md`. Takes a
//! [`CandidateBundle`] from Phase 4e, spawns a configured plugin
//! process, pipes the bundle in on stdin, parses the response off
//! stdout, surfaces any plugin error.
//!
//! The plugin can be any executable: a Rust bin, a Python script,
//! a Bash wrapper around `curl`. The boundary is JSON over a
//! single stdin/stdout exchange.
//!
//! For zero-config operation BHDL ships `bhdl-plugin-default`
//! (the binary defined in `src/bin/bhdl_plugin_default.rs`); the
//! analyzer's `default_plugin_command()` returns the right
//! `Command` to invoke it when the user hasn't set up anything
//! else.

use std::io::Write;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::catalog_scan::{bundle_to_json, CandidateBundle};

// ─────────────────────────────────────────────────────────────────
// Output schema (§7.4)
// ─────────────────────────────────────────────────────────────────

/// The plugin's reply, one per BHDL invocation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginResponse {
    pub protocol_version: String,
    #[serde(default)]
    pub selections: Vec<PluginSelection>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// One per `selections_needed[i]` entry in the input bundle.
/// Either a successful pick (`mpn` set) or a per-class error
/// (`error` set).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginSelection {
    pub class_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mpn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_sku: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qty: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_time_weeks: Option<u32>,
    /// Real published ESR (ohms) for this part, when the provider/catalogue
    /// carries it (electrolytic/tantalum/polymer caps). Real-Data Policy: this
    /// is a measured per-MPN value, never an estimate — absent for ceramics
    /// (DigiKey carries no ceramic ESR/DF), which keeps stability UNCHECKED.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub esr_ohms: Option<f64>,
    /// Frequency (Hz) the ESR is specified at, when stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub esr_test_freq_hz: Option<f64>,
    /// The selected part's dielectric / temperature-coefficient code (e.g.
    /// `"X7R"`, `"C0G"`) for ceramics. Real per-MPN data — lets sign-off
    /// identify a ceramic output cap (structurally low ESR) without an estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dielectric: Option<String>,
    /// The selected part's rated power (watts), when the provider/catalogue
    /// carries it (resistors). Real per-MPN data — stamped as the instance's
    /// `power_rating` so sign-off checks the dissipation margin against the
    /// actual part, not the stdlib family's fallback stamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_rating_w: Option<f64>,
    /// The selected part's rated voltage (volts), when the provider/catalogue
    /// carries it (capacitors). Real per-MPN data — stamped as the instance's
    /// `voltage_rating` so sign-off checks the voltage margin against the
    /// actual part, not the stdlib family's fallback stamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_rating_v: Option<f64>,
    /// The selected part's rated current (amps), when the provider/catalogue
    /// carries it (inductors; conservative min of Irms/Isat). Real per-MPN
    /// data — stamped as the instance's `current_rating` so sign-off checks
    /// the current margin against the actual part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_rating_a: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl PluginSelection {
    /// True if this selection represents a successful pick.
    pub fn is_ok(&self) -> bool { self.error.is_none() && self.mpn.is_some() }
}

// ─────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum PluginError {
    /// Failed to spawn the plugin process at all (binary not
    /// found, permission denied, etc.).
    Spawn(std::io::Error),
    /// Plugin exited non-zero before returning a parseable JSON
    /// response. `stderr` is captured for the diagnostic.
    NonZeroExit { code: i32, stderr: String },
    /// Plugin's stdout could not be parsed as JSON matching
    /// PluginResponse.
    BadResponse { stderr: String, source: serde_json::Error },
    /// Plugin and BHDL disagree on protocol version.
    ProtocolMismatch { theirs: String, ours: &'static str },
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Spawn(e) => write!(f, "plugin spawn failed: {}", e),
            PluginError::NonZeroExit { code, stderr } => write!(
                f, "plugin exited with code {}\nstderr:\n{}", code, stderr.trim()
            ),
            PluginError::BadResponse { source, stderr } => {
                if stderr.trim().is_empty() {
                    write!(f, "plugin returned unparseable JSON: {}", source)
                } else {
                    write!(f, "plugin returned unparseable JSON: {}\nstderr:\n{}",
                           source, stderr.trim())
                }
            }
            PluginError::ProtocolMismatch { theirs, ours } => write!(
                f, "plugin protocol {} ≠ bhdl protocol {}", theirs, ours
            ),
        }
    }
}

impl std::error::Error for PluginError {}

// ─────────────────────────────────────────────────────────────────
// Invocation
// ─────────────────────────────────────────────────────────────────

const PROTOCOL_VERSION: &str = "1";

/// Run a configured plugin against a bundle.
///
/// `command` is a fully constructed [`Command`] — typically built
/// from `bhdl.toml` config — that the user controls. For zero-
/// config callers, see [`default_plugin_command`].
pub fn run_plugin(
    bundle: &CandidateBundle,
    mut command: Command,
) -> Result<PluginResponse, PluginError> {
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().map_err(PluginError::Spawn)?;

    let payload = bundle_to_json(bundle);
    if let Some(mut stdin) = child.stdin.take() {
        // It's important to close stdin so the plugin sees EOF
        // and exits. We `drop` after writing, which closes the pipe.
        stdin.write_all(payload.as_bytes()).map_err(PluginError::Spawn)?;
        drop(stdin);
    }

    let output = child.wait_with_output().map_err(PluginError::Spawn)?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(PluginError::NonZeroExit { code, stderr });
    }

    let response: PluginResponse = serde_json::from_slice(&output.stdout)
        .map_err(|source| PluginError::BadResponse { source, stderr: stderr.clone() })?;

    if response.protocol_version != PROTOCOL_VERSION {
        return Err(PluginError::ProtocolMismatch {
            theirs: response.protocol_version,
            ours: PROTOCOL_VERSION,
        });
    }

    Ok(response)
}

/// Build a [`Command`] for the bundled default plugin.
///
/// The default plugin lives as `bhdl-plugin-default` in the same
/// cargo workspace. In a `cargo run` / `cargo test` context the
/// path resolves under `target/debug/`; users can also `cargo
/// install` it onto their PATH.
pub fn default_plugin_command() -> Command {
    Command::new(default_plugin_binary_path())
}

/// Locate the default-plugin binary. Searches:
///   1. `BHDL_DEFAULT_PLUGIN_PATH` env var (test/CI override).
///   2. `target/debug/bhdl_plugin_default`  (cargo-run convention).
///   3. `target/release/bhdl_plugin_default`.
///   4. Bare `bhdl-plugin-default` on PATH.
pub fn default_plugin_binary_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("BHDL_DEFAULT_PLUGIN_PATH") {
        return std::path::PathBuf::from(p);
    }
    let candidates = [
        "target/debug/bhdl_plugin_default",
        "target/release/bhdl_plugin_default",
        "../target/debug/bhdl_plugin_default",
        "../target/release/bhdl_plugin_default",
    ];
    for c in &candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() { return p; }
    }
    std::path::PathBuf::from("bhdl_plugin_default")
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_scan::{run_catalog_scan, InstanceClass};
    use crate::part_family::ClassInstance;
    use bhdl_ast::SourceFile;
    use bhdl_common::ConstValue;
    use bhdl_parser::parse;
    use rowan::ast::AstNode;
    use std::fs;

    fn load(path: &str) -> (SourceFile, String) {
        let content = fs::read_to_string(path).unwrap();
        let pr = parse(&content);
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        (sf, path.to_string())
    }

    /// End-to-end: build a bundle, invoke the default plugin,
    /// parse its response, assert the expected MPN was picked.
    /// Requires the default plugin to be built — the test harness
    /// builds it first via `cargo build --bin bhdl-plugin-default`.
    #[test]
    fn default_plugin_picks_first_alphabetical() {
        // Ensure the default-plugin bin is built.
        let status = std::process::Command::new("cargo")
            .args(["build", "--quiet", "--bin", "bhdl_plugin_default"])
            .status()
            .expect("cargo build");
        assert!(status.success(), "cargo build of default plugin failed");

        let catalog = vec![
            load("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl"),
            load("../bhdl-stdlib/parts/panasonic/erj_3ek.bhdl"),
            load("../bhdl-stdlib/parts/avx/cr0603_fx.bhdl"),
        ];
        let instances = vec![InstanceClass {
            refdes: "R1".to_string(),
            class: ClassInstance {
                entity: "Resistor".to_string(),
                generics: vec![
                    ConstValue::Resistance(10_000.0),
                    ConstValue::String("1%".to_string()),
                    ConstValue::String("0603".to_string()),
                ],
            },
        }];
        let bundle = run_catalog_scan("test_board", &instances, &catalog);

        let response = run_plugin(&bundle, default_plugin_command())
            .expect("plugin should succeed");

        assert_eq!(response.protocol_version, "1");
        assert_eq!(response.selections.len(), 1);
        let sel = &response.selections[0];
        assert!(sel.is_ok());
        assert_eq!(sel.class_index, 0);

        // Alphabetical by manufacturer: AVX < Panasonic < Yageo.
        // AVX_CR0603_FX's MPN template is "CR0603-FX-{e96_code(R)}ELF".
        // With R=10kΩ, e96_code → "1002", so MPN should be
        // CR0603-FX-1002ELF.
        assert_eq!(sel.mpn.as_deref(), Some("CR0603-FX-1002ELF"));
        assert_eq!(sel.qty, Some(1));
    }

    #[test]
    fn no_candidates_yields_per_class_error() {
        let status = std::process::Command::new("cargo")
            .args(["build", "--quiet", "--bin", "bhdl_plugin_default"])
            .status()
            .expect("cargo build");
        assert!(status.success());

        let catalog = vec![load("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl")];
        let instances = vec![InstanceClass {
            refdes: "R99".to_string(),
            class: ClassInstance {
                entity: "Resistor".to_string(),
                generics: vec![
                    ConstValue::Resistance(0.1),  // out of range
                    ConstValue::String("1%".to_string()),
                    ConstValue::String("0603".to_string()),
                ],
            },
        }];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        let response = run_plugin(&bundle, default_plugin_command()).unwrap();

        let sel = &response.selections[0];
        assert!(!sel.is_ok());
        assert_eq!(sel.error.as_deref(), Some("no_candidates"));
        assert!(sel.mpn.is_none());
    }
}
