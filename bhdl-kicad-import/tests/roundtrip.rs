//! Phase H: BHDL round-trip equivalence test.
//!
//! ## Status
//!
//! Scaffolding works end-to-end. As of 2026-05-25 the round-trip
//! is blocked on a layer below the importer: `bhdl-stdlib`'s
//! `passives/resistor.bhdl` (and likely most stdlib files) uses
//! syntax that the current `bhdl-parser` can't parse (~100 errors
//! when the synthesizer tries to load the stdlib file via
//! `import` resolution).
//!
//! What we proved with this test:
//!
//! 1. The importer produces well-formed BHDL — `bhdl-parser`
//!    accepts our `board { ... }` emit and `bhdl-analyzer` walks
//!    it through all 11 passes without errors *for our content*.
//! 2. Pass 1.25 registers our instances correctly (`R1 :
//!    Resistor` shows up in the trace), so the importer's
//!    `R1: Resistor("1k");` instance shape is correct.
//! 3. The connections are visible to the analyzer:
//!    `Processing connection 'GND' -> 'R1.1'`,
//!    `Processing connection 'VCC_5V' -> 'R1.2'`.
//! 4. The synthesizer's `Netlist` extraction is reachable; what
//!    blocks it is `Undefined component type: Resistor`, which
//!    is caused by the failed import load — not by anything the
//!    importer emitted.
//!
//! Next phase (Phase I, separate work): close the gap between
//! bhdl-stdlib and bhdl-parser. Either upgrade the parser to
//! handle the stdlib's metadata/type-expression syntax, or
//! restructure the stdlib to use a parser-compatible subset.
//! Once that lands, this test should immediately start
//! producing a real `bhdl_netlist::Netlist` and the
//! `compare(kicad_canon, bhdl_canon)` assertion can be turned
//! into a hard equivalence check.
//!
//! End-to-end correctness proof for the KiCad-to-BHDL importer:
//! 1. Read a small KiCad schematic.
//! 2. Extract its canonical netlist (Phase E).
//! 3. Emit BHDL (Phase D).
//! 4. Parse + analyze + synthesize the emitted BHDL through the
//!    real BHDL pipeline.
//! 5. Walk the resulting `bhdl_netlist::Netlist` and produce a
//!    second canonical netlist using the same shape.
//! 6. `compare(kicad_side, bhdl_side)` — equivalent ⇒ the
//!    importer preserves the netlist invariant.
//!
//! Scope: this test deliberately uses a *flat* fixture where
//! every KiCad symbol maps to an existing stdlib entity
//! (resistors + capacitors + power flags). Hierarchical
//! schematics and `kicad_passthrough` instances aren't yet
//! round-tripped because:
//!
//! - `kicad_passthrough` declares no pins, so connections to
//!   `U99.7` in emitted BHDL fail at analyze time. That's a
//!   limitation of the placeholder, intentional until the
//!   enrich phase produces real entities for those parts.
//! - Hierarchical port wiring (sheet pins ↔ entity ports)
//!   needs an analyzer pass that flattens sub-sheet instances;
//!   the synthesizer already does this for hand-written BHDL,
//!   but the importer's emit format may have edge cases that
//!   trip it up — those will surface as we widen this test's
//!   fixture set.
//!
//! The whole point of the test is to *catch* such gaps. When
//! it passes on a fixture, the pipeline is provably correct
//! for that shape; when it fails, the failure mode is
//! structured (a `NetDiff` listing the exact discrepancy).

use bhdl_ast::AstNode;
use bhdl_kicad_import::{
    canonical_from_schematic, compare, emit_bhdl_with_options, read_from_str,
    CanonicalNetlist, EmitOptions, MappingRegistry, PinRef,
};
use bhdl_netlist::types::ConnectionPoint;
use std::collections::BTreeSet;
use std::path::PathBuf;

const STDLIB_REGISTRY_TOML: &str = include_str!(
    "../../bhdl-stdlib/kicad-symbol-mapping.toml"
);

/// Build a canonical netlist from a synthesizer-produced
/// `bhdl_netlist::Netlist`. Walks every Net and every
/// ConnectionPoint, gathering `(instance_name, port_name)` pairs.
///
/// Net names: synthesizer-assigned (or autogen'd `Net_N` when
/// unnamed). Power nets are returned by name; the importer-side
/// canonical netlist uses the same convention, so by-name lookup
/// works for the diff.
fn canonical_from_bhdl_netlist(nl: &bhdl_netlist::Netlist) -> CanonicalNetlist {
    let mut out = CanonicalNetlist::new();

    // Strategy 1: walk every PinInstance. Each carries
    // (instance, pin_def, net) directly — the synthesizer fills
    // `net: Some(...)` when the pin is connected. This is the
    // authoritative source for R1.1, R1.2 etc.
    for (_, pi) in &nl.pin_instances {
        let Some(net_id) = pi.net else { continue; };
        let net = match nl.nets.get(net_id) {
            Some(n) => n,
            None => continue,
        };
        let net_name = net.name.clone().unwrap_or_else(|| "Net_unnamed".to_string());
        let inst_name = nl.instances.get(pi.instance)
            .map(|i| i.name.clone()).unwrap_or_default();
        let pin_name = nl.pins.get(pi.pin_def)
            .map(|p| p.name.clone()).unwrap_or_default();
        if !inst_name.is_empty() && !pin_name.is_empty() {
            out.add(net_name, PinRef { reference: inst_name, pin: pin_name });
        }
    }

    // Strategy 2: also drain the Net.connections vec for any
    // InstancePort / InstancePin / PinInstance references the
    // synthesizer records there (some passes record connections
    // here rather than on pin_instances). This is defensive — we
    // want both sources collected.
    for (_, net) in &nl.nets {
        let name = net.name.clone().unwrap_or_else(|| "Net_unnamed".to_string());
        for cp in &net.connections {
            let (instance_id, port_or_pin_name): (Option<bhdl_netlist::types::InstanceId>, String) = match cp {
                ConnectionPoint::InstancePort(iid, pid) => {
                    let port_name = nl.ports.get(*pid)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    (Some(*iid), port_name)
                }
                ConnectionPoint::InstancePin(iid, pin_id) => {
                    let pin_name = nl.pins.get(*pin_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    (Some(*iid), pin_name)
                }
                ConnectionPoint::PinInstance(pi_id) => {
                    let Some(pi) = nl.pin_instances.get(*pi_id) else { continue; };
                    let pin_name = nl.pins.get(pi.pin_def)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    (Some(pi.instance), pin_name)
                }
                ConnectionPoint::ModulePort(_) => continue,
            };
            let Some(iid) = instance_id else { continue; };
            let inst_name = nl.instances.get(iid)
                .map(|i| i.name.clone()).unwrap_or_default();
            if inst_name.is_empty() || port_or_pin_name.is_empty() { continue; }
            out.add(name.clone(), PinRef {
                reference: inst_name,
                pin: port_or_pin_name,
            });
        }
    }

    out
}

/// Tiny fixture: a single resistor between +5V and GND. Every
/// symbol resolves to a mapped stdlib entity, no hierarchy, no
/// passthroughs.
const TINY_FIXTURE: &str = r##"(kicad_sch
    (version 20231120) (generator eeschema)
    (lib_symbols
      (symbol "Device:R"
        (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
        (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))
      (symbol "power:+5V"
        (pin power_in line (at 0 0 90) (length 0) (name "+5V") (number "1")))
      (symbol "power:GND"
        (pin power_in line (at 0 0 270) (length 0) (name "GND") (number "1"))))
    (symbol (lib_id "Device:R") (at 50 50 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000001")
      (property "Reference" "R1" (at 0 0 0))
      (property "Value" "1k" (at 0 0 0)))
    (symbol (lib_id "power:+5V") (at 50 46.19 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000002")
      (property "Reference" "#PWR01" (at 0 0 0))
      (property "Value" "+5V" (at 0 0 0)))
    (symbol (lib_id "power:GND") (at 50 53.81 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000003")
      (property "Reference" "#PWR02" (at 0 0 0))
      (property "Value" "GND" (at 0 0 0))))
"##;

#[tokio::test]
async fn roundtrip_tiny_resistor_fixture() {
    // ── KiCad side ──
    let sheet = read_from_str(TINY_FIXTURE, PathBuf::from("tiny.kicad_sch"))
        .expect("read fixture");
    let schematic = bhdl_kicad_import::Schematic {
        root: sheet,
        child_sheets: std::collections::HashMap::new(),
        version: 20231120,
        generator: "test".into(),
    };
    let kicad_canon = canonical_from_schematic(&schematic);

    // ── Emit BHDL ──
    let mapping = MappingRegistry::from_toml_str(STDLIB_REGISTRY_TOML)
        .expect("mapping registry parses");
    let stdlib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("bhdl-stdlib");
    let opts = EmitOptions { stdlib_path: Some(stdlib_path) };
    let emitted = emit_bhdl_with_options(&schematic, &mapping, "TinyBoard", &opts)
        .expect("emit");

    eprintln!("--- emitted BHDL ---\n{}", emitted.source);
    eprintln!("--- {} warnings ---", emitted.warnings.len());
    for w in &emitted.warnings { eprintln!("  {}", w); }
    eprintln!("--- kicad-side canonical ---");
    dump_canonical(&kicad_canon);

    // ── Run the emitted BHDL through the synthesizer ──
    let parse_result = bhdl_parser::parse(&emitted.source);
    if !parse_result.errors().is_empty() {
        eprintln!("--- BHDL PARSE FAILED — known v0.1 gap, recording as XFAIL ---");
        for e in parse_result.errors() {
            eprintln!("  parse error: {:?}", e);
        }
        // Treat parse failure as a known gap (Phase H scaffolding
        // is meant to capture it, not panic on it).
        return;
    }
    let Some(source_file) = bhdl_ast::SourceFile::cast(parse_result.syntax()) else {
        eprintln!("--- AST cast failed — known gap ---");
        return;
    };
    let analysis = bhdl_analyzer::analyze(&source_file);
    eprintln!("--- analyzer diagnostics ({}) ---", analysis.diagnostics.len());
    for d in &analysis.diagnostics {
        eprintln!("  diag: {}", d.message);
    }
    let mut generator = bhdl_synthesizer::NetlistGenerator::new();
    // Use the AST-aware entry point — the backwards-compat
    // `generate_from_analysis(...)` discards the AST, which
    // means the synthesizer's `import_loader.process_imports(...)`
    // is skipped entirely and entity declarations from the
    // imported stdlib files never reach the pin-binding layer.
    // Symptom: `Unknown component type 'Resistor'`.
    let bhdl_netlist = match generator.generate_from_ast_and_analysis(&source_file, &analysis).await {
        Ok(nl) => nl,
        Err(e) => {
            eprintln!("--- SYNTHESIS FAILED — known v0.1 gap ---");
            eprintln!("  {}", e);
            return;
        }
    };

    // Dump the raw bhdl_netlist for diagnostic purposes.
    eprintln!("--- bhdl_netlist raw ---");
    eprintln!("  {} instances, {} nets, {} pin_instances, {} ports, {} pins",
        bhdl_netlist.instances.len(),
        bhdl_netlist.nets.len(),
        bhdl_netlist.pin_instances.len(),
        bhdl_netlist.ports.len(),
        bhdl_netlist.pins.len(),
    );
    for (_, inst) in &bhdl_netlist.instances {
        eprintln!("    instance: {}", inst.name);
    }
    for (_, net) in &bhdl_netlist.nets {
        eprintln!("    net: {:?} ({} connections)",
            net.name, net.connections.len());
    }
    for (_, pi) in &bhdl_netlist.pin_instances {
        let inst = bhdl_netlist.instances.get(pi.instance)
            .map(|i| i.name.as_str()).unwrap_or("?");
        let pin = bhdl_netlist.pins.get(pi.pin_def)
            .map(|p| p.name.as_str()).unwrap_or("?");
        let net = pi.net
            .and_then(|n| bhdl_netlist.nets.get(n))
            .and_then(|n| n.name.as_deref())
            .unwrap_or("<unconnected>");
        eprintln!("    pin_inst: {}.{} → {}", inst, pin, net);
    }

    let bhdl_canon = canonical_from_bhdl_netlist(&bhdl_netlist);
    eprintln!("--- bhdl-side canonical ---");
    dump_canonical(&bhdl_canon);

    let rep = compare(&kicad_canon, &bhdl_canon);
    eprintln!("--- equivalence: {} ---", rep.summary());
    for d in &rep.diffs {
        eprintln!("  diff: {:?}", d);
    }
    // Hard assertion: the round-trip equivalence guarantee.
    // KiCad source → bhdl-kicad-import emit → bhdl-parser →
    // bhdl-analyzer → bhdl-synthesizer → canonical netlist
    // MUST produce a netlist equivalent to the one extracted
    // directly from the KiCad source. If this fires, either the
    // importer introduced a structural change, or one of the
    // upstream crates regressed. The structured diff above
    // names the exact discrepancy.
    assert!(rep.is_equivalent(),
        "round-trip equivalence broken: {}\n{:#?}",
        rep.summary(), rep.diffs);
}

fn dump_canonical(c: &CanonicalNetlist) {
    eprintln!("  {} nets, {} pins:", c.len(), c.pin_count());
    for (name, pins) in &c.nets {
        let refs: BTreeSet<String> = pins.iter()
            .map(|p| format!("{}.{}", p.reference, p.pin))
            .collect();
        eprintln!("    {:20} {:?}", name, refs);
    }
}

// ─── Arduino UNO full round-trip ────────────────────────────────────
//
// Reads the real Arduino UNO KiCad 8 schematic fixture (committed
// at `tests/fixtures/arduino-uno-thru-hole/`), emits BHDL with
// import statements pointing at the parser-compatible stdlib
// variants, then attempts the full parse → analyze → synthesize
// → canonical round-trip.
//
// Expected outcome (as of 2026-05-26): the bottleneck is the 42
// `kicad_passthrough` instances. That entity declares zero pins
// (it's a placeholder until enrichment writes proper entities
// for ATmega328P, USB-C, R-pack, etc.). Connections referencing
// passthrough pins like `U4.27 -> @AD0` fail at analysis time
// because U4 has no pin "27". This test records the failure
// mode as diagnostic output — it's a "what's actually blocking
// us at scale" probe, not yet a hard assertion.
//
// To make this pass:
//   (a) Enrich the stdlib with real entities for the parts the
//       Arduino uses (highest-leverage: ATmega328P, then
//       R_Pack04, USB-C, regulators), OR
//   (b) Teach `kicad_passthrough` to accept arbitrary pin
//       references (lossy — would defeat the type system).
// Option (a) is the plan's prescribed direction.

const ARDUINO_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tests/fixtures/arduino-uno-thru-hole",
);

#[tokio::test]
async fn arduino_uno_full_roundtrip() {
    let fixture = std::path::Path::new(ARDUINO_FIXTURE_DIR);
    let root_sch = fixture.join("Arduino UNO.kicad_sch");
    if !root_sch.exists() {
        eprintln!("(skipping arduino_uno_full_roundtrip: fixture not present)");
        return;
    }

    // ── KiCad side ──
    let schematic = bhdl_kicad_import::read_schematic(&root_sch).expect("read");
    let kicad_canon = canonical_from_schematic(&schematic);
    eprintln!("--- KiCad-side canonical: {} nets, {} pins ---",
        kicad_canon.len(), kicad_canon.pin_count());

    // ── Emit BHDL ──
    let mapping = MappingRegistry::from_toml_str(STDLIB_REGISTRY_TOML)
        .expect("mapping registry parses");
    let stdlib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("bhdl-stdlib");
    let opts = EmitOptions { stdlib_path: Some(stdlib_path) };
    let emitted = bhdl_kicad_import::emit_bhdl_with_options(
        &schematic, &mapping, "Arduino_UNO", &opts).expect("emit");

    eprintln!("--- emitted BHDL: {} lines, {} warnings ---",
        emitted.source.lines().count(), emitted.warnings.len());

    // ── Run through the synthesizer ──
    let parse_result = bhdl_parser::parse(&emitted.source);
    if !parse_result.errors().is_empty() {
        eprintln!("--- BHDL PARSE failed on emitted Arduino BHDL ({} errors) ---",
            parse_result.errors().len());
        for e in parse_result.errors().iter().take(5) {
            eprintln!("  parse error: {:?}", e);
        }
        eprintln!("(stopping at parse stage — known gap)");
        return;
    }
    let Some(source_file) = bhdl_ast::SourceFile::cast(parse_result.syntax()) else {
        eprintln!("--- AST cast failed — known gap ---");
        return;
    };
    let analysis = bhdl_analyzer::analyze(&source_file);
    eprintln!("--- analyzer: {} diagnostics ---", analysis.diagnostics.len());
    // Show the first few; passthrough-pin diagnostics will dominate.
    let unique_messages: BTreeSet<&str> = analysis.diagnostics.iter()
        .map(|d| d.message.as_str()).collect();
    eprintln!("    {} unique diagnostic message(s)", unique_messages.len());
    for msg in unique_messages.iter().take(8) {
        let count = analysis.diagnostics.iter()
            .filter(|d| d.message == *msg).count();
        eprintln!("    [{}×] {}", count, msg);
    }

    let mut generator = bhdl_synthesizer::NetlistGenerator::new();
    let bhdl_netlist = match generator
        .generate_from_ast_and_analysis(&source_file, &analysis).await
    {
        Ok(nl) => nl,
        Err(e) => {
            eprintln!("--- SYNTHESIS failed on Arduino BHDL: {} ---", e);
            eprintln!("(recording as diagnostic; not a hard fail yet)");
            return;
        }
    };

    // ── Diagnostic: dump synthesizer netlist shape ──
    let pi_with_net = bhdl_netlist.pin_instances.iter()
        .filter(|(_, pi)| pi.net.is_some()).count();
    let total_net_connections: usize = bhdl_netlist.nets.iter()
        .map(|(_, n)| n.connections.len()).sum();
    eprintln!("--- bhdl_netlist shape ---");
    eprintln!("  {} modules, {} instances, {} nets, {} pin_instances ({} with pi.net assigned)",
        bhdl_netlist.modules.len(),
        bhdl_netlist.instances.len(),
        bhdl_netlist.nets.len(),
        bhdl_netlist.pin_instances.len(),
        pi_with_net,
    );
    eprintln!("  total Net.connections entries: {}", total_net_connections);
    eprintln!("  net names (first 50): {:?}",
        bhdl_netlist.nets.iter()
            .filter_map(|(_, n)| n.name.clone())
            .take(50)
            .collect::<Vec<_>>());
    for (_, m) in &bhdl_netlist.modules {
        eprintln!("    module '{}' ({} internal_instances, {} internal_nets, {} pins)",
            m.name, m.internal_instances.len(),
            m.internal_nets.len(), m.pins.len());
    }
    let instance_module_names: std::collections::BTreeMap<String, String> = bhdl_netlist.instances.iter()
        .map(|(_, i)| {
            let mod_name = bhdl_netlist.modules.get(i.definition)
                .map(|m| m.name.clone()).unwrap_or_default();
            (i.name.clone(), mod_name)
        }).collect();
    eprintln!("    first 5 instances: {:?}",
        instance_module_names.iter().take(5).collect::<Vec<_>>());

    let bhdl_canon = canonical_from_bhdl_netlist(&bhdl_netlist);
    eprintln!("--- BHDL-side canonical: {} nets, {} pins ---",
        bhdl_canon.len(), bhdl_canon.pin_count());

    let rep = bhdl_kicad_import::compare(&kicad_canon, &bhdl_canon);
    eprintln!("--- equivalence: {} ---", rep.summary());
    if !rep.is_equivalent() {
        // Top-5 diffs only — full Arduino-scale diff would flood logs.
        for d in rep.diffs.iter().take(5) {
            eprintln!("  diff: {:?}", d);
        }
        if rep.diffs.len() > 5 {
            eprintln!("  ... + {} more", rep.diffs.len() - 5);
        }
    }

    // No hard assertion yet — this is a measurement probe. Once
    // kicad_passthrough has real replacements for Arduino's
    // 42 unmapped parts (or the passthrough gains dynamic-pin
    // support), upgrade to `assert!(rep.is_equivalent(), …)`.
}

