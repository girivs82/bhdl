//! KiCad → BHDL symbol-mapping registry.
//!
//! Phase B of the KiCad-to-BHDL translator. Reads
//! `kicad-symbol-mapping.toml` (shipped with `bhdl-stdlib`) and
//! answers: "given KiCad lib_id `Device:R`, what BHDL entity should
//! I instantiate and how do I bind its pins?"
//!
//! The registry is intentionally loose: missing entries fall through
//! to `kicad_passthrough`, which produces weaker-but-valid BHDL.
//! This lets the importer make progress on real boards before every
//! IC has a stdlib entry — the "translate then enrich" strategy from
//! the plan.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Errors specific to the mapping registry.
#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    #[error("file I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// One row in `kicad-symbol-mapping.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolMapping {
    /// `"Library:Symbol"` as it appears in the schematic `lib_id`.
    pub kicad: String,
    /// Target BHDL entity name (e.g. `"Resistor"`). The special
    /// `"_net:NAME"` form means "this isn't an entity, it names a
    /// power net" — the importer maps power-flag symbols this way.
    pub bhdl: String,
    /// KiCad-pin-number → BHDL-port-name. Missing entries default
    /// to identity mapping (pin name == port name).
    #[serde(default)]
    pub pin_map: HashMap<String, String>,
}

impl SymbolMapping {
    /// True when this mapping points at a virtual power net, not a
    /// real component to instantiate.
    pub fn is_power_net(&self) -> bool { self.bhdl.starts_with("_net:") }

    /// Strip the `_net:` prefix for power-net mappings; returns the
    /// canonical BHDL net name (e.g. `"GND"`, `"VCC_5V"`).
    pub fn power_net_name(&self) -> Option<&str> {
        self.bhdl.strip_prefix("_net:")
    }

    /// Translate a KiCad pin number to a BHDL port name. Falls back
    /// to identity when no explicit map exists.
    pub fn translate_pin<'a>(&'a self, kicad_number: &'a str) -> &'a str {
        self.pin_map
            .get(kicad_number)
            .map(|s| s.as_str())
            .unwrap_or(kicad_number)
    }
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    mapping: Vec<SymbolMapping>,
}

/// Loaded mapping registry. Lookups are by full KiCad lib_id.
pub struct MappingRegistry {
    by_lib_id: HashMap<String, SymbolMapping>,
}

impl MappingRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self { by_lib_id: HashMap::new() }
    }

    /// Parse a TOML source string into a registry.
    pub fn from_toml_str(src: &str) -> Result<Self, MappingError> {
        let file: RegistryFile = toml::from_str(src)?;
        let mut by_lib_id = HashMap::with_capacity(file.mapping.len());
        for m in file.mapping {
            by_lib_id.insert(m.kicad.clone(), m);
        }
        Ok(Self { by_lib_id })
    }

    /// Read the registry from a TOML file on disk.
    pub fn from_toml_file(path: &Path) -> Result<Self, MappingError> {
        let src = std::fs::read_to_string(path)?;
        Self::from_toml_str(&src)
    }

    /// Look up a mapping by KiCad lib_id. Returns `None` for
    /// unmapped symbols — callers should fall through to
    /// `kicad_passthrough` in that case.
    pub fn lookup(&self, lib_id: &str) -> Option<&SymbolMapping> {
        self.by_lib_id.get(lib_id)
    }

    /// Number of registered mappings.
    pub fn len(&self) -> usize { self.by_lib_id.len() }

    /// Iterate over all loaded mappings (e.g. for diagnostics).
    pub fn iter(&self) -> impl Iterator<Item = &SymbolMapping> {
        self.by_lib_id.values()
    }

    /// Merge another registry on top of this one. Later entries win.
    /// Useful for project-local overrides on top of the bhdl-stdlib
    /// default.
    pub fn extend(&mut self, other: MappingRegistry) {
        for (k, v) in other.by_lib_id {
            self.by_lib_id.insert(k, v);
        }
    }
}

impl Default for MappingRegistry {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_registry() {
        let src = r#"
            [[mapping]]
            kicad = "Device:R"
            bhdl  = "resistor"
            pin_map = { "1" = "p", "2" = "n" }

            [[mapping]]
            kicad = "Device:LED"
            bhdl  = "led"
            pin_map = { "1" = "k", "2" = "a" }
        "#;
        let reg = MappingRegistry::from_toml_str(src).expect("parse");
        assert_eq!(reg.len(), 2);

        let r = reg.lookup("Device:R").expect("found");
        assert_eq!(r.bhdl, "resistor");
        assert_eq!(r.translate_pin("1"), "p");
        assert_eq!(r.translate_pin("2"), "n");

        let led = reg.lookup("Device:LED").expect("found");
        assert_eq!(led.translate_pin("1"), "k");
        // Unmapped pin numbers fall through to identity.
        assert_eq!(led.translate_pin("99"), "99");
    }

    #[test]
    fn power_net_mappings_recognised() {
        let src = r#"
            [[mapping]]
            kicad = "power:GND"
            bhdl  = "_net:GND"

            [[mapping]]
            kicad = "power:+5V"
            bhdl  = "_net:VCC_5V"
        "#;
        let reg = MappingRegistry::from_toml_str(src).expect("parse");
        let gnd = reg.lookup("power:GND").expect("found");
        assert!(gnd.is_power_net());
        assert_eq!(gnd.power_net_name(), Some("GND"));

        let v5 = reg.lookup("power:+5V").expect("found");
        assert!(v5.is_power_net());
        assert_eq!(v5.power_net_name(), Some("VCC_5V"));
    }

    #[test]
    fn missing_pin_map_defaults_to_empty() {
        let src = r#"
            [[mapping]]
            kicad = "Connector:USB_C_Receptacle"
            bhdl  = "connector_usb_c"
        "#;
        let reg = MappingRegistry::from_toml_str(src).expect("parse");
        let m = reg.lookup("Connector:USB_C_Receptacle").expect("found");
        assert!(m.pin_map.is_empty());
        // Identity mapping for every pin.
        assert_eq!(m.translate_pin("VBUS"), "VBUS");
        assert_eq!(m.translate_pin("GND"), "GND");
    }

    #[test]
    fn unmapped_lookup_returns_none() {
        let reg = MappingRegistry::from_toml_str("").expect("parse");
        assert!(reg.lookup("Some:Unknown").is_none());
    }

    #[test]
    fn loads_real_stdlib_registry() {
        // Locate the registry shipped with bhdl-stdlib. This test
        // doubles as a check that the file is well-formed TOML.
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("bhdl-stdlib").join("kicad-symbol-mapping.toml"));
        if let Some(p) = candidate {
            if p.exists() {
                let reg = MappingRegistry::from_toml_file(&p).expect("load registry");
                // Spot-check a couple of well-known entries.
                assert!(reg.lookup("Device:R").is_some());
                assert!(reg.lookup("Device:C").is_some());
                assert!(reg.lookup("power:GND").is_some());
                assert!(reg.len() >= 30, "expected at least 30 entries, got {}", reg.len());
            }
        }
    }
}
