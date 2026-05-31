//! Cargo-style library resolution for BHDL.
//!
//! Resolves `import { … } from "<namespace>/<path>.bhdl"` statements
//! against **declared** libraries — the bundled `bhdl-stdlib`,
//! third-party, and proprietary/internal libs — in a reproducible way.
//!
//! Two manifests (see `docs/spec/Library_Resolution.md`):
//!   - **project** `bhdl.toml` (`[project]`, `[libraries]`) — declares
//!     *which* libraries a board depends on, by name + version. Version-
//!     controlled with the board.
//!   - **library** `manifest.toml` (`[library] name, version`) — marks a
//!     directory as a library root and declares its identity.
//!
//! Declaration (manifest) is separate from location (a `path =` entry,
//! or a search path supplied via CLI `-I` / `$BHDL_LIB_PATH`). Imports
//! resolve *only* against declared+version-matched libraries, so a
//! build is reproducible: the search path can only say *where* a
//! declared lib lives, never introduce an undeclared one.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The reserved namespace for the bundled standard library. Always
/// available without a `[libraries]` entry — BHDL's "std".
pub const STDLIB_NAMESPACE: &str = "bhdl-stdlib";

/// Project manifest — `bhdl.toml`, version-controlled next to the board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub project: ProjectInfo,
    #[serde(default)]
    pub libraries: HashMap<String, Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// A declared library dependency. Accepts two TOML shapes:
///   `lib = "1.4"`                       (bare version, name-resolved)
///   `lib = { version = "1.4" }`         (name-resolved)
///   `lib = { path = "../x", version = "2.1" }`  (explicit root)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Bare version string shorthand.
    Version(String),
    /// Table form.
    Detailed {
        version: String,
        #[serde(default)]
        path: Option<String>,
    },
}

impl Dependency {
    pub fn version(&self) -> &str {
        match self {
            Dependency::Version(v) => v,
            Dependency::Detailed { version, .. } => version,
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            Dependency::Version(_) => None,
            Dependency::Detailed { path, .. } => path.as_deref(),
        }
    }
}

/// Library-root manifest — `manifest.toml` at each library root.
/// Only `[library]` name + version are required for resolution;
/// any other tables (`[components]`, `[compatibility]`) are ignored
/// here so a minimal proprietary lib needs only the `[library]` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryManifest {
    pub library: LibraryRootInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRootInfo {
    pub name: String,
    pub version: String,
}

/// Walk up from `start_dir` looking for a `bhdl.toml`. Mirrors Cargo's
/// manifest discovery. Returns the path to the manifest file, if found.
pub fn discover_project_manifest(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = Some(start_dir);
    while let Some(d) = dir {
        let candidate = d.join("bhdl.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

impl ProjectManifest {
    /// Parse a `bhdl.toml` from disk.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {}", path.display(), e))?;
        toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("parsing {}: {}", path.display(), e))
    }
}

impl LibraryManifest {
    /// Parse a library-root `manifest.toml`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {}", path.display(), e))?;
        toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("parsing {}: {}", path.display(), e))
    }
}

/// Exact-with-patch-flexibility version match (v0 semantics): the
/// declared `req` matches `have` when their major.minor agree. A bare
/// `"2"` matches any `2.x.y`; `"2.1"` matches any `2.1.z`; `"2.1.3"`
/// requires exact. Full semver ranges are a v1 addition.
pub fn version_matches(req: &str, have: &str) -> bool {
    let r: Vec<&str> = req.split('.').collect();
    let h: Vec<&str> = have.split('.').collect();
    // Compare only as many components as the requirement specifies.
    for (i, rc) in r.iter().enumerate() {
        match h.get(i) {
            Some(hc) if hc == rc => continue,
            _ => return false,
        }
    }
    true
}

/// Resolves namespaced import paths (`<ns>/<rel>.bhdl`) to files on
/// disk against a project manifest + search roots. Built once per
/// build and shared by the import loader and the component-library
/// resolver (the two formerly-disjoint mechanisms).
#[derive(Debug, Clone, Default)]
pub struct LibraryResolver {
    /// The project manifest's declared dependencies (None → only
    /// `bhdl-stdlib` imports are permitted; back-compat for
    /// stdlib-only boards with no `bhdl.toml`).
    manifest: Option<ProjectManifest>,
    /// Directory the manifest lives in (for resolving `path =` entries).
    manifest_dir: Option<PathBuf>,
    /// Library search roots, highest precedence first: `-I`/`--lib-path`
    /// dirs (CLI order), then `$BHDL_LIB_PATH` entries.
    search_roots: Vec<PathBuf>,
    /// Location of the bundled `bhdl-stdlib` (resolved separately).
    stdlib_root: Option<PathBuf>,
}

/// Outcome of resolving an import's namespace to a library root.
#[derive(Debug)]
pub enum ResolveError {
    /// `<ns>` is not `bhdl-stdlib` and not declared in `[libraries]`.
    Undeclared { namespace: String },
    /// Declared, but no root on the search path / `path =` matched.
    NotFound { namespace: String, searched: Vec<PathBuf> },
    /// Found a root, but its `manifest.toml` version disagrees with the
    /// declared requirement.
    VersionMismatch { namespace: String, required: String, found: String, root: PathBuf },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Undeclared { namespace } => write!(
                f,
                "import references library `{namespace}`, which is not declared in \
                 bhdl.toml [libraries] (only `{STDLIB_NAMESPACE}` is implicit)"
            ),
            ResolveError::NotFound { namespace, searched } => write!(
                f,
                "library `{namespace}` is declared but no matching root was found. \
                 Searched: {}",
                searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            ResolveError::VersionMismatch { namespace, required, found, root } => write!(
                f,
                "library `{namespace}` at {} is version {found}, but bhdl.toml requires {required}",
                root.display()
            ),
        }
    }
}
impl std::error::Error for ResolveError {}

impl LibraryResolver {
    /// Build a resolver. `manifest_path` is an optional `bhdl.toml`;
    /// `cli_roots` are `-I`/`--lib-path` dirs (highest precedence);
    /// `BHDL_LIB_PATH` is appended after them; `stdlib_root` locates the
    /// bundled lib.
    pub fn new(
        manifest_path: Option<&Path>,
        cli_roots: &[PathBuf],
        env_lib_path: Option<&str>,
        stdlib_root: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let (manifest, manifest_dir) = match manifest_path {
            Some(p) => {
                let m = ProjectManifest::load(p)?;
                (Some(m), p.parent().map(|d| d.to_path_buf()))
            }
            None => (None, None),
        };

        let mut search_roots: Vec<PathBuf> = cli_roots.to_vec();
        if let Some(env) = env_lib_path {
            for part in env.split(':').filter(|s| !s.is_empty()) {
                search_roots.push(PathBuf::from(part));
            }
        }

        Ok(LibraryResolver { manifest, manifest_dir, search_roots, stdlib_root })
    }

    /// Resolve an import `from "<ns>/<rel>.bhdl"` to an absolute file
    /// path. `./` and `../` imports are NOT handled here — the caller
    /// keeps its existing file-relative behaviour for those.
    pub fn resolve_import(&self, import_path: &str) -> Result<PathBuf, ResolveError> {
        let (ns, rel) = match import_path.split_once('/') {
            Some(x) => x,
            None => (import_path, ""),
        };

        let root = self.resolve_namespace_root(ns)?;
        Ok(if rel.is_empty() { root } else { root.join(rel) })
    }

    /// Resolve just the library namespace to its root directory.
    pub fn resolve_namespace_root(&self, ns: &str) -> Result<PathBuf, ResolveError> {
        // 1. The bundled stdlib is always available.
        if ns == STDLIB_NAMESPACE {
            if let Some(root) = &self.stdlib_root {
                return Ok(root.clone());
            }
            // No explicit stdlib root configured: treat the namespace
            // dir as a literal relative path (today's behaviour).
            return Ok(PathBuf::from(ns));
        }

        // 2. Must be declared.
        let dep = self
            .manifest
            .as_ref()
            .and_then(|m| m.libraries.get(ns))
            .ok_or_else(|| ResolveError::Undeclared { namespace: ns.to_string() })?;

        // 3a. Explicit path = root (relative to the manifest dir).
        if let Some(p) = dep.path() {
            let root = match &self.manifest_dir {
                Some(d) => d.join(p),
                None => PathBuf::from(p),
            };
            return self.version_check(ns, dep.version(), root);
        }

        // 3b. Name-resolved: scan search roots for a manifest.toml whose
        // [library] name matches, with a compatible version.
        let mut searched = Vec::new();
        for base in &self.search_roots {
            // A root may itself be the lib dir, or a parent containing
            // <ns>/ as a subdir. Try both.
            for candidate in [base.clone(), base.join(ns)] {
                searched.push(candidate.clone());
                let mpath = candidate.join("manifest.toml");
                if let Ok(lm) = LibraryManifest::load(&mpath) {
                    if lm.library.name == ns
                        && version_matches(dep.version(), &lm.library.version)
                    {
                        return Ok(candidate);
                    }
                }
            }
        }
        Err(ResolveError::NotFound { namespace: ns.to_string(), searched })
    }

    fn version_check(&self, ns: &str, required: &str, root: PathBuf) -> Result<PathBuf, ResolveError> {
        let mpath = root.join("manifest.toml");
        match LibraryManifest::load(&mpath) {
            Ok(lm) if version_matches(required, &lm.library.version) => Ok(root),
            Ok(lm) => Err(ResolveError::VersionMismatch {
                namespace: ns.to_string(),
                required: required.to_string(),
                found: lm.library.version,
                root,
            }),
            // No/invalid manifest at an explicit path: treat as not-found
            // so the error names the path the user pointed at.
            Err(_) => Err(ResolveError::NotFound {
                namespace: ns.to_string(),
                searched: vec![root],
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_dependency_shapes() {
        let m: ProjectManifest = toml::from_str(
            r#"
            [project]
            name = "demo"
            version = "0.1.0"

            [libraries]
            acme-stdlib = { path = "../acme/acme-stdlib", version = "2.1" }
            sensor-lib  = { version = "1.4" }
            fpga-lib    = "0.9"
            "#,
        )
        .unwrap();
        assert_eq!(m.project.name, "demo");
        assert_eq!(m.libraries["acme-stdlib"].path(), Some("../acme/acme-stdlib"));
        assert_eq!(m.libraries["acme-stdlib"].version(), "2.1");
        assert_eq!(m.libraries["sensor-lib"].path(), None);
        assert_eq!(m.libraries["fpga-lib"].version(), "0.9");
        assert_eq!(m.libraries["fpga-lib"].path(), None);
    }

    #[test]
    fn version_match_semantics() {
        assert!(version_matches("2.1", "2.1.0"));
        assert!(version_matches("2.1", "2.1.9"));
        assert!(version_matches("2", "2.7.3"));
        assert!(version_matches("2.1.3", "2.1.3"));
        assert!(!version_matches("2.1", "2.2.0"));
        assert!(!version_matches("2.1.3", "2.1.4"));
        assert!(!version_matches("3", "2.1.0"));
    }

    // ── resolver filesystem tests ────────────────────────────────
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Unique temp dir for a test (avoids a tempfile dependency).
    fn tmp() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("bhdl_libres_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Create a library root `<base>/<name>/` with a manifest.toml and
    /// a single entity file. Returns the lib root dir.
    fn make_lib(base: &Path, name: &str, version: &str) -> PathBuf {
        let root = base.join(name);
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(
            root.join("manifest.toml"),
            format!("[library]\nname = \"{name}\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
        std::fs::write(root.join("parts/widget.bhdl"), "entity Widget { pin A: signal inout; }\n").unwrap();
        root
    }

    fn write_manifest(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("bhdl.toml");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn resolves_via_explicit_path() {
        let t = tmp();
        let libs = t.join("libs");
        std::fs::create_dir_all(&libs).unwrap();
        make_lib(&libs, "acme-stdlib", "2.1.0");
        // manifest dir is `t`; path is relative to it.
        let mp = write_manifest(
            &t,
            "[project]\nname=\"b\"\n[libraries]\nacme-stdlib = { path = \"libs/acme-stdlib\", version = \"2.1\" }\n",
        );
        let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
        let f = r.resolve_import("acme-stdlib/parts/widget.bhdl").unwrap();
        assert!(f.ends_with("acme-stdlib/parts/widget.bhdl"));
        assert!(f.is_file());
    }

    #[test]
    fn resolves_via_dash_i_root() {
        let t = tmp();
        let libs = t.join("cli_libs");
        std::fs::create_dir_all(&libs).unwrap();
        make_lib(&libs, "sensor-lib", "1.4.2");
        let mp = write_manifest(
            &t,
            "[project]\nname=\"b\"\n[libraries]\nsensor-lib = \"1.4\"\n",
        );
        let r = LibraryResolver::new(Some(&mp), &[libs.clone()], None, None).unwrap();
        let f = r.resolve_import("sensor-lib/parts/widget.bhdl").unwrap();
        assert!(f.is_file());
    }

    #[test]
    fn resolves_via_env_path() {
        let t = tmp();
        let libs = t.join("env_libs");
        std::fs::create_dir_all(&libs).unwrap();
        make_lib(&libs, "fpga-lib", "0.9.0");
        let mp = write_manifest(
            &t,
            "[project]\nname=\"b\"\n[libraries]\nfpga-lib = { version = \"0.9\" }\n",
        );
        let env = libs.to_string_lossy().to_string();
        let r = LibraryResolver::new(Some(&mp), &[], Some(&env), None).unwrap();
        assert!(r.resolve_import("fpga-lib/parts/widget.bhdl").unwrap().is_file());
    }

    #[test]
    fn undeclared_namespace_errors() {
        let t = tmp();
        let mp = write_manifest(&t, "[project]\nname=\"b\"\n");
        let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
        match r.resolve_import("rogue/x.bhdl") {
            Err(ResolveError::Undeclared { namespace }) => assert_eq!(namespace, "rogue"),
            other => panic!("expected Undeclared, got {:?}", other),
        }
    }

    #[test]
    fn version_mismatch_errors() {
        let t = tmp();
        let libs = t.join("libs");
        std::fs::create_dir_all(&libs).unwrap();
        make_lib(&libs, "acme-stdlib", "3.0.0"); // lib is 3.0, manifest wants 2.1
        let mp = write_manifest(
            &t,
            "[project]\nname=\"b\"\n[libraries]\nacme-stdlib = { path = \"libs/acme-stdlib\", version = \"2.1\" }\n",
        );
        let r = LibraryResolver::new(Some(&mp), &[], None, None).unwrap();
        match r.resolve_import("acme-stdlib/parts/widget.bhdl") {
            Err(ResolveError::VersionMismatch { required, found, .. }) => {
                assert_eq!(required, "2.1");
                assert_eq!(found, "3.0.0");
            }
            other => panic!("expected VersionMismatch, got {:?}", other),
        }
    }

    #[test]
    fn stdlib_namespace_always_available() {
        // No manifest at all → only bhdl-stdlib resolvable, as a literal.
        let r = LibraryResolver::new(None, &[], None, None).unwrap();
        let f = r.resolve_import("bhdl-stdlib/actives/foo.bhdl").unwrap();
        assert_eq!(f, PathBuf::from("bhdl-stdlib/actives/foo.bhdl"));
        // …and a non-stdlib import with no manifest is Undeclared.
        assert!(matches!(r.resolve_import("acme/x.bhdl"), Err(ResolveError::Undeclared { .. })));
    }
}
