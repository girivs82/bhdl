//! Default BOM-selection plugin.
//!
//! Reads a Phase-4e CandidateBundle JSON document from stdin and
//! writes a PluginResponse JSON document to stdout. Selection
//! policy: pick the first candidate per class, ordered by
//! `(manufacturer, mpn)` ascending. Deterministic, no network,
//! no policy hints. Just enough to produce a valid BOM out-of-
//! the-box so users can see the pipeline working without
//! configuring their own plugin.
//!
//! Per-class error if `candidates` is empty:
//!   { "class_index": N, "error": "no_candidates",
//!     "message": "no part_family matched this class" }
//!
//! See `docs/spec/Parameterization_And_BOM_Resolution.md` §7 for
//! the protocol; this bin is the reference implementation.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
struct CandidateBundle {
    #[allow(dead_code)]
    bhdl_version: String,
    #[allow(dead_code)]
    protocol_version: String,
    #[allow(dead_code)]
    board: String,
    selections_needed: Vec<ClassSelection>,
}

#[derive(Deserialize)]
struct ClassSelection {
    #[allow(dead_code)]
    class: String,
    #[allow(dead_code)]
    #[serde(default)]
    instance_count: usize,
    #[allow(dead_code)]
    #[serde(default)]
    instances: Vec<String>,
    #[serde(default)]
    candidates: Vec<Candidate>,
}

#[derive(Deserialize, Clone)]
struct Candidate {
    family: String,
    mpn: String,
    #[serde(default)]
    manufacturer: Option<String>,
}

#[derive(Serialize)]
struct PluginResponse {
    protocol_version: String,
    selections: Vec<PluginSelection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct PluginSelection {
    class_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    mpn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn main() -> io::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let bundle: CandidateBundle = match serde_json::from_str(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("bhdl-plugin-default: could not parse input JSON: {}", e);
            std::process::exit(2);
        }
    };

    let mut selections = Vec::new();
    for (i, sel) in bundle.selections_needed.iter().enumerate() {
        if sel.candidates.is_empty() {
            selections.push(PluginSelection {
                class_index: i,
                mpn: None,
                manufacturer: None,
                family: None,
                qty: None,
                note: None,
                error: Some("no_candidates".to_string()),
                message: Some(format!(
                    "no part_family matched class {} for instances {:?}",
                    sel.class, sel.instances
                )),
            });
            continue;
        }

        // Pick first by (manufacturer, mpn) ascending. Use empty
        // string as fallback for sorting when manufacturer is None.
        let mut sorted: Vec<&Candidate> = sel.candidates.iter().collect();
        sorted.sort_by(|a, b| {
            let am = a.manufacturer.as_deref().unwrap_or("");
            let bm = b.manufacturer.as_deref().unwrap_or("");
            am.cmp(bm).then(a.mpn.cmp(&b.mpn))
        });
        let winner = sorted[0];

        selections.push(PluginSelection {
            class_index: i,
            mpn: Some(winner.mpn.clone()),
            manufacturer: winner.manufacturer.clone(),
            family: Some(winner.family.clone()),
            qty: Some(sel.instance_count.max(1)),
            note: Some("deterministic default selection by (manufacturer, mpn)".to_string()),
            error: None,
            message: None,
        });
    }

    let response = PluginResponse {
        protocol_version: "1".to_string(),
        selections,
        warnings: Vec::new(),
    };

    let out = serde_json::to_string_pretty(&response).expect("serialize");
    io::stdout().write_all(out.as_bytes())?;
    io::stdout().write_all(b"\n")?;
    Ok(())
}
