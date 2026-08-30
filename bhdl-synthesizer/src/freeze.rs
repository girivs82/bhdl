//! Frozen structural netlist — the immutable "as-fabbed" record.
//!
//! Where the lockfile (`bhdl.lock`) makes a build *reproducible from
//! source*, a frozen netlist captures the *result*: every concrete
//! component (with its resolved value/footprint) and the flat
//! connectivity, after all expansion/design/parametric inference has
//! run — but WITHOUT the recipes, intents, or parametric templates that
//! produced it. It's the manufacturing/release record: "this exact
//! netlist is what we fabbed," self-describing and dependency-pinned,
//! and it does not depend on any library being retrievable later.
//!
//! This is a *derived snapshot*, not rebuildable source: it has no
//! design layer to re-run. Pair it with the lockfile (reproducible
//! rebuild) — the two are complementary, per the reproducibility
//! discussion in docs/spec/Library_Resolution.md.
//!
//! The schema is **stable and versioned** (unlike `synth --format json`,
//! which dumps the internal `Netlist` verbatim). Archive it, diff it,
//! feed it to a board house.

use bhdl_netlist::Netlist;
use bhdl_common::library::LockedLibrary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current frozen-netlist schema version. Bumped if the shape changes;
/// readers check it.
pub const FROZEN_SCHEMA_VERSION: u32 = 1;

/// The complete frozen record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenNetlist {
    pub schema_version: u32,
    pub provenance: Provenance,
    /// Concrete components, sorted by refdes for stable diffs.
    pub components: Vec<FrozenComponent>,
    /// Nets, sorted by name for stable diffs.
    pub nets: Vec<FrozenNet>,
}

/// Self-describing build context: what produced this artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// BHDL toolchain version that emitted this.
    pub tool_version: String,
    /// Source `.bhdl` file the board was built from.
    pub source: String,
    /// ISO-8601 timestamp (supplied by the caller).
    pub generated_at: String,
    /// The libraries (name + exact version + content hash) this build
    /// resolved against — the lockfile contents, embedded so the
    /// frozen record alone documents its full dependency set.
    #[serde(default)]
    pub libraries: Vec<LockedLibrary>,
}

/// One concrete component as fabbed: resolved value/footprint, no
/// recipe/intent/template metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenComponent {
    pub refdes: String,
    pub name: String,
    pub component_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpn: Option<String>,
    /// Remaining attributes (resolved values), curated to structural /
    /// BOM-relevant keys and sorted. Synthesis-internal and
    /// placement-intent keys are excluded — this is the as-fabbed
    /// structural record, not the design model.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

/// One net: name, class, and the flat list of pins it connects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenNet {
    pub name: String,
    pub net_class: String,
    /// `(refdes, pin)` endpoints, sorted.
    pub connections: Vec<FrozenPin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrozenPin {
    pub refdes: String,
    pub pin: String,
}

/// Attribute keys that are synthesis-internal or design-layer metadata
/// and must NOT appear in the as-fabbed structural record. Anything
/// matching these prefixes/names is dropped from `attributes`.
fn is_internal_attr(key: &str) -> bool {
    key.starts_with("intf_")          // interface constraint/binding/dir/xwire
        || key.starts_with("intf_const")
        || key.starts_with("vpin_")       // virtual-pin provenance
        || key.starts_with("expansion_")  // expansion provenance
        || key.starts_with("alias__")     // function aliases
        || key == "abstract_origin"
        || key == "selected_sku"          // (kept? — it's provenance; drop from structural)
        || key == "socketed_in"
}

/// Build a [`FrozenNetlist`] from a synthesized netlist + provenance.
pub fn freeze_netlist(netlist: &Netlist, provenance: Provenance) -> FrozenNetlist {
    // ── Components ────────────────────────────────────────────────
    let mut components: Vec<FrozenComponent> = Vec::new();
    for (_id, inst) in netlist.instances.iter() {
        // Definition-template stubs (`Res: Res`, template=true and
        // unconnected — see is_template_stub) are analyzer scaffolding,
        // not parts: the as-fabbed record must not carry phantoms.
        if crate::is_template_stub(netlist, _id) {
            continue;
        }
        let component_type = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // refdes: explicit attribute if present, else the instance name.
        let refdes = inst
            .attributes
            .get("refdes")
            .cloned()
            .unwrap_or_else(|| inst.name.clone());

        let value = inst.attributes.get("value").cloned();
        let footprint = inst
            .attributes
            .get("footprint")
            .or_else(|| inst.attributes.get("package"))
            .cloned();
        let mpn = inst
            .attributes
            .get("mpn")
            .or_else(|| inst.attributes.get("part_number"))
            .or_else(|| inst.attributes.get("part_no"))
            .cloned();

        let mut attributes = BTreeMap::new();
        for (k, v) in &inst.attributes {
            if is_internal_attr(k) {
                continue;
            }
            // Already surfaced as first-class fields.
            if matches!(k.as_str(), "refdes" | "value" | "footprint" | "package" | "mpn") {
                continue;
            }
            attributes.insert(k.clone(), v.clone());
        }

        components.push(FrozenComponent {
            refdes,
            name: inst.name.clone(),
            component_type,
            value,
            footprint,
            mpn,
            attributes,
        });
    }
    components.sort_by(|a, b| a.refdes.cmp(&b.refdes).then(a.name.cmp(&b.name)));

    // ── Nets ──────────────────────────────────────────────────────
    // Group pin-instances by their net (the same flattening the
    // interface tests use): for each net, collect (refdes, pin).
    let mut nets: Vec<FrozenNet> = Vec::new();
    for (net_id, net) in netlist.nets.iter() {
        let name = net
            .name
            .clone()
            .unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut connections: Vec<FrozenPin> = Vec::new();
        for (_pi_id, pi) in netlist.pin_instances.iter() {
            if pi.net != Some(net_id) {
                continue;
            }
            let inst = match netlist.instances.get(pi.instance) {
                Some(i) => i,
                None => continue,
            };
            let pin = match netlist.pins.get(pi.pin_def) {
                Some(p) => p.name.clone(),
                None => continue,
            };
            let refdes = inst.attributes.get("refdes").cloned().unwrap_or_else(|| inst.name.clone());
            connections.push(FrozenPin { refdes, pin });
        }
        connections.sort();
        connections.dedup();
        if connections.is_empty() {
            continue; // skip dangling/internal nets with no real pins
        }
        nets.push(FrozenNet {
            name,
            net_class: format!("{:?}", net.net_class),
            connections,
        });
    }
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    FrozenNetlist {
        schema_version: FROZEN_SCHEMA_VERSION,
        provenance,
        components,
        nets,
    }
}
