//! Vendor-file store and manifest — the distribution story for
//! simulation data that legally cannot ship with the repository.
//!
//! Stdlib entities reference vendor IBIS files by repo-relative path
//! (`vendor/ibis/megaavr/m16u2m32.ibs`). Those files are typically
//! licensed "free to use and modify, do not redistribute" (Atmel/
//! Microchip IBIS), so the repo commits only a MANIFEST — expected
//! path, sha256, where to obtain it, license summary — and the USER
//! obtains the files. `bhdl vendor install` verifies a user-supplied
//! download against the manifest and places it in a local store;
//! the simulation path resolves references through that store.
//!
//! Search order for a reference `P` (first hit wins):
//!   1. `P` as written (absolute, or relative to the working directory
//!      — the in-repo development case),
//!   2. `<board dir>/P` (board-local .ibs fixtures),
//!   3. `<store>/P` and `<store>/P-minus-leading-"vendor/"`,
//!      where `<store>` = `$BHDL_VENDOR_DIR`, else `~/.bhdl/vendor`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The committed manifest: `vendor/MANIFEST.toml` at the repo root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorManifest {
    #[serde(default, rename = "file")]
    pub files: Vec<VendorFile>,
}

/// One expected vendor file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorFile {
    /// Repo-relative reference path, exactly as stdlib entities write it.
    pub path: String,
    /// SHA-256 of the exact file the stdlib was validated against.
    pub sha256: String,
    /// Where the user obtains it (vendor page / download name) — a
    /// description, not a URL that will rot.
    pub source: String,
    /// License summary explaining WHY it isn't committed.
    pub license: String,
    /// Stdlib entities that reference it.
    #[serde(default)]
    pub used_by: Vec<String>,
}

impl VendorManifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("bad manifest {}: {e}", path.display()))
    }

    /// Walk upward from `start` looking for `vendor/MANIFEST.toml`.
    pub fn find_upwards(start: &Path) -> Option<PathBuf> {
        let mut dir = if start.is_dir() { start } else { start.parent()? };
        loop {
            let candidate = dir.join("vendor/MANIFEST.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = dir.parent()?;
        }
    }
}

/// The local vendor store root: `$BHDL_VENDOR_DIR`, else `~/.bhdl/vendor`.
pub fn store_root() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("BHDL_VENDOR_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".bhdl/vendor"))
}

/// Where `manifest_path` (a repo-relative reference like
/// `vendor/ibis/x.ibs`) lives INSIDE the store: the same path with the
/// leading `vendor/` stripped, so the store mirrors `vendor/` itself.
pub fn store_relpath(manifest_path: &str) -> &str {
    manifest_path.strip_prefix("vendor/").unwrap_or(manifest_path)
}

/// Resolve a vendor-file reference through the search order. Returns
/// the first existing candidate, or None (caller decides how to report
/// the absence).
pub fn resolve(reference: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let raw = PathBuf::from(reference);
    if raw.exists() {
        return Some(raw);
    }
    if let Some(base) = base_dir {
        let p = base.join(reference);
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(store) = store_root() {
        let p = store.join(reference);
        if p.exists() {
            return Some(p);
        }
        let p = store.join(store_relpath(reference));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(format!("{:x}", h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_relpath_strips_vendor_prefix() {
        assert_eq!(store_relpath("vendor/ibis/a/b.ibs"), "ibis/a/b.ibs");
        assert_eq!(store_relpath("other/x.ibs"), "other/x.ibs");
    }

    #[test]
    fn manifest_roundtrip() {
        let text = r#"
[[file]]
path = "vendor/ibis/megaavr/m16u2m32.ibs"
sha256 = "abc"
source = "somewhere"
license = "use-not-redistribute"
used_by = ["bhdl-stdlib/actives/atmega16u2.bhdl"]
"#;
        let m: VendorManifest = toml::from_str(text).unwrap();
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].used_by.len(), 1);
    }

    #[test]
    fn resolve_prefers_as_written_then_base_then_store() {
        let tmp = std::env::temp_dir().join(format!("bhdl_vendor_test_{}", std::process::id()));
        let base = tmp.join("base");
        let store = tmp.join("store");
        std::fs::create_dir_all(base.join("vendor/ibis")).unwrap();
        std::fs::create_dir_all(store.join("ibis")).unwrap();
        std::fs::write(base.join("vendor/ibis/f.ibs"), "base").unwrap();
        std::fs::write(store.join("ibis/f.ibs"), "store").unwrap();

        // Env-dependent branch: point the store at our temp dir.
        std::env::set_var("BHDL_VENDOR_DIR", &store);

        // base_dir hit wins over the store...
        let p = resolve("vendor/ibis/f.ibs", Some(&base)).unwrap();
        assert!(p.starts_with(&base), "expected base hit, got {}", p.display());
        // ...and without base, the store's stripped layout resolves.
        let p = resolve("vendor/ibis/f.ibs", None).unwrap();
        assert!(p.starts_with(&store), "expected store hit, got {}", p.display());

        std::env::remove_var("BHDL_VENDOR_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sha256_matches_known_vector() {
        let tmp = std::env::temp_dir().join(format!("bhdl_sha_test_{}", std::process::id()));
        std::fs::write(&tmp, "abc").unwrap();
        // NIST test vector for "abc".
        assert_eq!(
            sha256_file(&tmp).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&tmp);
    }
}
