//! Reference Designator LUT — persistent handle → refdes mapping.
//!
//! A sidecar `.bhdl.refdes` JSON file alongside each `.bhdl` source persists
//! the mapping from user-declared handles (e.g. `r_load`) to standard
//! reference designators (e.g. `R1`). Handles are the human namespace —
//! user-authored, arbitrarily long, stable in source/nets/logs; refdes is
//! the fab namespace — allocated ONCE by the synthesizer (phase 4.7),
//! stamped as the `refdes` instance attribute, and read (never minted) by
//! every consumer: schematic, BOM, sign-off, freeze, ERC plugins, PnR.
//! The sidecar is a COMMITTED artifact (like a lockfile): once a handle
//! gets a refdes it keeps it even as parts are added or removed.

use std::collections::BTreeMap;
use std::path::Path;
use serde::{Serialize, Deserialize};

/// Persistent handle → refdes mapping, grouped by prefix. BTreeMap, not
/// HashMap: a committed lockfile must serialize deterministically, or
/// every synthesis run rewrites every sidecar with key-order noise.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RefDesLut {
    pub version: u32,
    /// prefix → (handle → refdes), e.g. "R" → {"r_load" → "R1", "r_led" → "R2"}
    pub mappings: BTreeMap<String, BTreeMap<String, String>>,
}

impl RefDesLut {
    /// Load a LUT from disk. Returns `Default` on missing file or parse error.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Write the LUT to disk as pretty-printed JSON. No-op when the file
    /// already holds identical content, so re-synthesis leaves mtimes alone.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        if std::fs::read_to_string(path).is_ok_and(|old| old == json) {
            return Ok(());
        }
        std::fs::write(path, json)
    }

    /// Assign a refdes for a handle. Returns the existing refdes if already
    /// mapped, otherwise assigns the next available number for the prefix.
    pub fn assign(&mut self, prefix: &str, handle: &str) -> String {
        let group = self.mappings.entry(prefix.to_string()).or_default();
        if let Some(existing) = group.get(handle) {
            return existing.clone();
        }
        // Find max number already used in this prefix group
        let max_num = group.values()
            .filter_map(|rd| rd.get(prefix.len()..).and_then(|s| s.parse::<usize>().ok()))
            .max()
            .unwrap_or(0);
        let refdes = format!("{}{}", prefix, max_num + 1);
        group.insert(handle.to_string(), refdes.clone());
        refdes
    }
}

