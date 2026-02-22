//! Reference Designator LUT — persistent handle → refdes mapping.
//!
//! A sidecar `.refdes` JSON file alongside each `.bhdl` source persists
//! the mapping from user-declared handles (e.g. `r_load`) to standard
//! reference designators (e.g. `R1`). This ensures refdes stability:
//! once a handle gets a refdes it keeps it even if other components
//! are added or removed.

use std::collections::HashMap;
use std::path::Path;
use serde::{Serialize, Deserialize};

/// Persistent handle → refdes mapping, grouped by prefix.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RefDesLut {
    pub version: u32,
    /// prefix → (handle → refdes), e.g. "R" → {"r_load" → "R1", "r_led" → "R2"}
    pub mappings: HashMap<String, HashMap<String, String>>,
}

impl RefDesLut {
    /// Load a LUT from disk. Returns `Default` on missing file or parse error.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Write the LUT to disk as pretty-printed JSON.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
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

/// Map a component category string to the standard refdes prefix.
pub fn category_to_prefix(category: &str) -> &str {
    match category {
        "resistor" => "R",
        "capacitor" => "C",
        "inductor" => "L",
        "diode" | "led" => "D",
        "protection" => "D",
        "regulator" | "ic" | "opamp" => "U",
        "buffer" => "U",
        "oscillator" => "Y",
        "connector" => "J",
        _ => "X",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assign_new_refdes() {
        let mut lut = RefDesLut::default();
        assert_eq!(lut.assign("R", "r_load"), "R1");
        assert_eq!(lut.assign("R", "r_led"), "R2");
        assert_eq!(lut.assign("C", "c_in"), "C1");
    }

    #[test]
    fn test_assign_existing_refdes() {
        let mut lut = RefDesLut::default();
        lut.assign("R", "r_load");
        // Second call returns same refdes
        assert_eq!(lut.assign("R", "r_load"), "R1");
    }

    #[test]
    fn test_skips_used_numbers() {
        let mut lut = RefDesLut::default();
        // Pre-populate with R1 and R3 (gap at R2)
        let group = lut.mappings.entry("R".to_string()).or_default();
        group.insert("r_a".to_string(), "R1".to_string());
        group.insert("r_b".to_string(), "R3".to_string());
        // Next assignment should be R4 (max+1)
        assert_eq!(lut.assign("R", "r_c"), "R4");
    }

    #[test]
    fn test_save_and_load() {
        let tmp = std::env::temp_dir().join("test_refdes_lut.json");
        let mut lut = RefDesLut { version: 1, ..Default::default() };
        lut.assign("R", "r_load");
        lut.assign("C", "c_in");
        lut.save(&tmp).unwrap();

        let loaded = RefDesLut::load(&tmp);
        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.mappings.get("R").and_then(|g| g.get("r_load")),
            Some(&"R1".to_string())
        );
        assert_eq!(
            loaded.mappings.get("C").and_then(|g| g.get("c_in")),
            Some(&"C1".to_string())
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_category_to_prefix() {
        assert_eq!(category_to_prefix("resistor"), "R");
        assert_eq!(category_to_prefix("capacitor"), "C");
        assert_eq!(category_to_prefix("inductor"), "L");
        assert_eq!(category_to_prefix("diode"), "D");
        assert_eq!(category_to_prefix("protection"), "D");
        assert_eq!(category_to_prefix("regulator"), "U");
        assert_eq!(category_to_prefix("ic"), "U");
        assert_eq!(category_to_prefix("unknown_thing"), "X");
    }
}
