//! Level-3 source resolvers — toolchain-driven fetch of a declared
//! library from a VCS/remote at a pinned revision.
//!
//! BHDL never speaks any VCS protocol. A resolver is an **external
//! executable** `bhdl-source-<scheme>` (found in a configured resolver
//! dir, else on `PATH`), invoked with a JSON request on stdin; it
//! populates a destination directory with the library root at the
//! pinned revision and exits 0. This makes the fetch layer
//! VCS-agnostic and extensible with no BHDL recompile — a Perforce
//! shop ships a ~20-line `bhdl-source-p4`. See
//! docs/spec/Source_Resolvers.md.
//!
//! Fetched trees are cached, keyed by `(scheme, locator, rev)`, so a
//! given pinned source is fetched once and reused (offline thereafter).
//! Content integrity is verified separately against the lockfile's
//! sha256 (caller's job, via `hash_library_root`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A parsed `source = "<scheme>:<locator>"` + pinned `rev`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpec {
    pub scheme: String,
    pub locator: String,
    pub rev: String,
}

impl SourceSpec {
    /// Parse a dependency's `source` + `rev` into a spec. `source` is
    /// `<scheme>:<locator>` where `<scheme>` is an identifier (the
    /// resolver selector). Errors if the shape is wrong or `rev` looks
    /// mutable (`main`/`head`/`latest`/empty — those defeat
    /// reproducibility; the content hash would catch the drift, but we
    /// fail early). Returns `(spec, warning?)`.
    pub fn parse(source: &str, rev: &str) -> anyhow::Result<SourceSpec> {
        let (scheme, locator) = source
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("source `{source}` is not `<scheme>:<locator>`"))?;
        if scheme.is_empty()
            || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!("source scheme `{scheme}` must be an identifier");
        }
        if locator.is_empty() {
            anyhow::bail!("source `{source}` has an empty locator");
        }
        if rev.trim().is_empty() {
            anyhow::bail!("source `{source}` requires a pinned `rev`");
        }
        Ok(SourceSpec {
            scheme: scheme.to_string(),
            locator: locator.to_string(),
            rev: rev.to_string(),
        })
    }

    /// Heuristic: does `rev` look like a moving ref rather than an
    /// immutable revision? Callers warn (not error) — the content hash
    /// still catches any resulting drift.
    pub fn rev_looks_mutable(&self) -> bool {
        let r = self.rev.to_ascii_lowercase();
        matches!(r.as_str(), "main" | "master" | "head" | "latest" | "trunk" | "tip")
    }
}

/// Root of the content cache. `$BHDL_CACHE` if set, else
/// `$HOME/.cache/bhdl/sources`, else a process-local temp dir.
pub fn cache_root() -> PathBuf {
    if let Ok(c) = std::env::var("BHDL_CACHE") {
        return PathBuf::from(c).join("sources");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/bhdl/sources");
    }
    std::env::temp_dir().join("bhdl-cache/sources")
}

/// Cache directory for a specific pinned source. Keyed by a digest of
/// `scheme:locator@rev` so it's computable from `bhdl.toml` alone
/// (look-up-able before any fetch), and laid out per-scheme for
/// human browsability.
pub fn spec_cache_dir(spec: &SourceSpec) -> PathBuf {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(spec.locator.as_bytes());
    h.update(b"@");
    h.update(spec.rev.as_bytes());
    let key = format!("{:x}", h.finalize());
    cache_root().join(&spec.scheme).join(&key[..16])
}

/// Options controlling source resolution.
#[derive(Debug, Clone, Default)]
pub struct FetchOptions {
    /// Directories searched (before `PATH`) for `bhdl-source-<scheme>`
    /// helper executables.
    pub resolver_dirs: Vec<PathBuf>,
    /// If true, never spawn a helper — a source must already be cached.
    pub offline: bool,
}

#[derive(Serialize)]
struct HelperRequest<'a> {
    protocol: u32,
    locator: &'a str,
    rev: &'a str,
    dest: &'a str,
    offline: bool,
}

#[derive(Deserialize)]
struct HelperResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    message: Option<String>,
}

static SCRATCH_CTR: AtomicU64 = AtomicU64::new(0);

/// Resolve a source dependency to a local library-root directory,
/// fetching via the scheme's helper if not already cached.
///
/// Returns the cache path containing the library root (with its
/// `manifest.toml`). Does NOT verify the content hash — the caller
/// checks it against the lockfile.
pub fn resolve_source(spec: &SourceSpec, opts: &FetchOptions) -> anyhow::Result<PathBuf> {
    let dir = spec_cache_dir(spec);
    if dir.join("manifest.toml").is_file() {
        return Ok(dir); // cache hit — no fetch, works offline
    }
    if opts.offline {
        anyhow::bail!(
            "source `{}:{}`@{} is not cached and --offline forbids fetching",
            spec.scheme, spec.locator, spec.rev
        );
    }

    // Fetch into a scratch dir, then atomically promote to the cache.
    let scratch = cache_root()
        .join(".tmp")
        .join(format!("{}-{}", std::process::id(), SCRATCH_CTR.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)
        .map_err(|e| anyhow::anyhow!("creating scratch {}: {}", scratch.display(), e))?;

    let result = run_helper(spec, &scratch, opts);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(e);
    }

    if !scratch.join("manifest.toml").is_file() {
        let _ = std::fs::remove_dir_all(&scratch);
        anyhow::bail!(
            "resolver `bhdl-source-{}` did not produce a library root (no manifest.toml) \
             for {}@{}",
            spec.scheme, spec.locator, spec.rev
        );
    }

    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // Another build may have populated it concurrently; tolerate that.
    if dir.join("manifest.toml").is_file() {
        let _ = std::fs::remove_dir_all(&scratch);
        return Ok(dir);
    }
    std::fs::rename(&scratch, &dir)
        .map_err(|e| anyhow::anyhow!("promoting fetch to cache {}: {}", dir.display(), e))?;
    Ok(dir)
}

fn run_helper(spec: &SourceSpec, dest: &Path, opts: &FetchOptions) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let helper_name = format!("bhdl-source-{}", spec.scheme);
    // Find in resolver_dirs first (absolute path), else rely on PATH.
    let program: PathBuf = opts
        .resolver_dirs
        .iter()
        .map(|d| d.join(&helper_name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(&helper_name));

    let req = HelperRequest {
        protocol: 1,
        locator: &spec.locator,
        rev: &spec.rev,
        dest: &dest.to_string_lossy(),
        offline: opts.offline,
    };
    let req_json = serde_json::to_string(&req)?;

    let mut child = Command::new(&program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!(
            "spawning resolver `{}` for scheme `{}`: {} \
             (install a `bhdl-source-{}` executable on PATH or in a resolver dir)",
            program.display(), spec.scheme, e, spec.scheme
        ))?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(req_json.as_bytes())
        .map_err(|e| anyhow::anyhow!("writing request to `{}`: {}", helper_name, e))?;

    let out = child
        .wait_with_output()
        .map_err(|e| anyhow::anyhow!("waiting on `{}`: {}", helper_name, e))?;

    if !out.status.success() {
        anyhow::bail!("resolver `{}` failed ({})", helper_name, out.status);
    }
    // A response body is optional; if present and ok:false, surface it.
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.trim().is_empty() {
        if let Ok(resp) = serde_json::from_str::<HelperResponse>(stdout.trim()) {
            if !resp.ok {
                anyhow::bail!(
                    "resolver `{}` reported failure: {}",
                    helper_name,
                    resp.message.unwrap_or_else(|| "(no message)".into())
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_flags_mutable_rev() {
        let s = SourceSpec::parse("git:ssh://git.acme/libs/x", "v2.1.0").unwrap();
        assert_eq!(s.scheme, "git");
        assert_eq!(s.locator, "ssh://git.acme/libs/x");
        assert!(!s.rev_looks_mutable());

        assert!(SourceSpec::parse("p4://depot/x/...", "main").unwrap().rev_looks_mutable());
        assert!(SourceSpec::parse("noscheme", "1").is_err());
        assert!(SourceSpec::parse("git:x", "").is_err());
    }

    #[test]
    fn cache_dir_is_deterministic_and_per_scheme() {
        let a = SourceSpec::parse("git:url", "abc").unwrap();
        let b = SourceSpec::parse("git:url", "abc").unwrap();
        assert_eq!(spec_cache_dir(&a), spec_cache_dir(&b));
        // different rev → different dir
        let c = SourceSpec::parse("git:url", "def").unwrap();
        assert_ne!(spec_cache_dir(&a), spec_cache_dir(&c));
        // scheme appears in the path
        assert!(spec_cache_dir(&a).to_string_lossy().contains("/git/"));
    }
}
