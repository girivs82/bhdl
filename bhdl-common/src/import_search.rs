//! Process-wide import search context.
//!
//! Direct-relative import strings (`import { X } from "bhdl-stdlib/…"`)
//! historically resolved as literal paths from the process working
//! directory, so running `bhdl <abs-board> …` from anywhere but the
//! workspace root failed with "Error loading import". This module gives
//! every import-resolution site (analyzer pass1, synthesizer
//! ImportLoader, LibraryResolver stdlib fallback) one shared lookup
//! order:
//!
//!   1. the importing file's directory (caller-supplied base)
//!   2. the input board's directory (set once by the CLI)
//!   3. `-I` / `--lib-path` roots, in CLI order (set once by the CLI)
//!   4. `$BHDL_LIB_PATH` entries
//!   5. the working directory (legacy literal path)
//!
//! The first candidate that exists on disk wins; when none exists the
//! legacy literal path is returned so error messages stay meaningful.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Default)]
struct SearchContext {
    /// Directory of the top-level input file (`bhdl <input>`).
    input_dir: Option<PathBuf>,
    /// `-I`/`--lib-path` roots in CLI order.
    cli_roots: Vec<PathBuf>,
}

static CONTEXT: OnceLock<SearchContext> = OnceLock::new();

/// Install the process-wide search context. Called once by the CLI
/// right after argument parsing; later calls are ignored (OnceLock).
pub fn set_search_context(input_dir: Option<PathBuf>, cli_roots: &[PathBuf]) {
    let _ = CONTEXT.set(SearchContext {
        input_dir: input_dir.filter(|d| !d.as_os_str().is_empty()),
        cli_roots: cli_roots.to_vec(),
    });
}

fn env_roots() -> Vec<PathBuf> {
    std::env::var("BHDL_LIB_PATH")
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// All search roots after the caller's base dir: input dir, `-I` roots,
/// then `$BHDL_LIB_PATH` entries.
fn context_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let ctx = CONTEXT.get();
    if let Some(d) = ctx.and_then(|c| c.input_dir.as_deref()) {
        roots.push(d.to_path_buf());
    }
    if let Some(c) = ctx {
        roots.extend(c.cli_roots.iter().cloned());
    }
    roots.extend(env_roots());
    // Lowest-precedence heuristic: the input dir's ancestors, so a
    // board deep inside a workspace (tests/circuits/…) finds the
    // workspace's bhdl-stdlib the same way manifest discovery walks up.
    if let Some(d) = ctx.and_then(|c| c.input_dir.as_deref()) {
        let mut dir = d;
        while let Some(p) = dir.parent() {
            if p.as_os_str().is_empty() {
                break;
            }
            roots.push(p.to_path_buf());
            dir = p;
        }
    }
    roots
}

/// Resolve a relative import string against the search order above.
/// Absolute paths pass through untouched. `base_dir` is the importing
/// file's directory (highest precedence). Returns the first existing
/// candidate, else the legacy fallback: `base_dir`-joined for `./` and
/// `../` imports, the literal cwd-relative path otherwise.
pub fn resolve_relative(import_path: &str, base_dir: &Path) -> PathBuf {
    let p = Path::new(import_path);
    if p.is_absolute() {
        return p.to_path_buf();
    }

    let base_candidate = base_dir.join(import_path);
    if base_candidate.exists() {
        return base_candidate;
    }
    for root in context_roots() {
        let candidate = root.join(import_path);
        if candidate.exists() {
            return candidate;
        }
    }
    if p.exists() {
        return p.to_path_buf();
    }

    // Nothing exists — preserve the legacy shape for error messages.
    if import_path.starts_with("./") || import_path.starts_with("../") {
        base_candidate
    } else {
        p.to_path_buf()
    }
}

/// Locate an existing directory by name (e.g. `bhdl-stdlib`) through
/// the same search order, without a caller base dir. Used to find the
/// bundled stdlib root when no explicit one is configured.
pub fn locate_dir(name: &str) -> Option<PathBuf> {
    for root in context_roots() {
        let candidate = root.join(name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    let literal = PathBuf::from(name);
    literal.is_dir().then_some(literal)
}
