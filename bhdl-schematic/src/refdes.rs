//! Schematic-side refdes surface. The LUT itself moved to
//! `bhdl_common::refdes` so the SYNTHESIZER can own allocation (phase 4.7,
//! stamping the `refdes` instance attribute); this module re-exports it and
//! keeps the schematic-category → prefix mapping.

pub use bhdl_common::refdes::RefDesLut;

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
        "connector" | "dc-jack" | "jack" | "header" | "usb" => "J",
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
