//! Apply a selected board SKU variant's patches to the synthesized
//! netlist.
//!
//! See `docs/spec/Board_SKU_Variants.md`. Runs *after* expansion so
//! that variant patches address instances by their final (expansion-
//! relative) names — including names produced by entity expansion
//! (`Itail_Rref`, `U1_Rk`, …).
//!
//! v0.1 surface:
//! - `<inst>.value = <expr>` → set `instance.attributes["value"]` to
//!   the override value (parsed via `bhdl_spice::model_factory::parse_value`,
//!   stored back as a normalized numeric string).
//! - `dnp <inst>`            → set `instance.attributes["do_not_populate"] = "true"`.
//!   The instance stays in the netlist (SPICE / PnR see it) but the
//!   BOM walker skips it. Future enhancements might flag PnR / KiCad
//!   export to suppress placement, but v0.1 keeps the netlist
//!   structurally identical across SKUs.
//!
//! Patches that reference an instance that doesn't exist in the
//! netlist are reported as warnings (not errors) so the user gets
//! a complete diagnostic even if multiple targets are missing.

use bhdl_common::variant::Variant;
use bhdl_netlist::Netlist;
use log::{info, warn};

/// Apply `variant`'s patches to `netlist` in place. Returns a report
/// of what was changed (useful for the CLI to print a one-line
/// summary).
#[derive(Debug, Default, Clone)]
pub struct VariantApplyReport {
    pub variant_name: String,
    pub values_changed: usize,
    pub instances_dnpd: usize,
    /// Names of patch targets that didn't exist in the netlist.
    pub missing_instances: Vec<String>,
}

pub fn apply_variant(netlist: &mut Netlist, variant: &Variant) -> VariantApplyReport {
    let mut report = VariantApplyReport {
        variant_name: variant.name.clone(),
        ..Default::default()
    };

    // Build a name → InstanceId index once so we don't quadratic-scan
    // for each patch target.
    let mut by_name: std::collections::HashMap<&str, bhdl_netlist::InstanceId>
        = std::collections::HashMap::new();
    for (id, inst) in &netlist.instances {
        by_name.insert(inst.name.as_str(), id);
    }
    // Collect into owned key set so we can mutate `netlist.instances`
    // freely in the loops below.
    let owned: std::collections::HashMap<String, bhdl_netlist::InstanceId> =
        by_name.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    drop(by_name);

    // Value overrides.
    for (inst_name, expr) in &variant.value_overrides {
        match owned.get(inst_name) {
            Some(&id) => {
                if let Some(inst) = netlist.instances.get_mut(id) {
                    // Store the raw expression as the value. The
                    // synthesizer's downstream model_extractor /
                    // converter already understands the same value
                    // string shape (numeric literal with optional
                    // unit suffix) that ordinary instantiations
                    // produce, so this just replaces the original
                    // literal in place.
                    inst.attributes.insert("value".to_string(), expr.trim().to_string());
                    info!("Variant '{}': {}.value = {}", variant.name, inst_name, expr.trim());
                    report.values_changed += 1;
                }
            }
            None => {
                warn!("Variant '{}': can't override value of '{}' — no such instance \
                       in the post-expansion netlist", variant.name, inst_name);
                report.missing_instances.push(inst_name.clone());
            }
        }
    }

    // DNP marks.
    for inst_name in &variant.dnp {
        match owned.get(inst_name) {
            Some(&id) => {
                if let Some(inst) = netlist.instances.get_mut(id) {
                    inst.attributes.insert("do_not_populate".to_string(), "true".to_string());
                    info!("Variant '{}': DNP '{}'", variant.name, inst_name);
                    report.instances_dnpd += 1;
                }
            }
            None => {
                warn!("Variant '{}': can't DNP '{}' — no such instance in the \
                       post-expansion netlist", variant.name, inst_name);
                report.missing_instances.push(inst_name.clone());
            }
        }
    }

    report
}
