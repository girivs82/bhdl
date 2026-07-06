//! Bare instance references in flow chains thread the chain THROUGH the
//! part in series (`VCC -> R1 -> LED1 -> GND`), instead of being mistaken
//! for net names — which left every part in the chain electrically
//! unconnected (the schematic-v4 "unidiomized" residue family:
//! test_simple_led, test_synthesizer).
//!
//! Also covers the polarized-terminal alias family (test_7805_*): a `.+` /
//! `.-` reference must connect to a part whose pins are declared `pos` /
//! `neg`.

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_netlist::Netlist;

async fn synthesize(source: &str) -> Netlist {
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    let mut generator = NetlistGenerator::new();
    generator
        .generate_from_ast_and_analysis(&source_file, &analysis)
        .await
        .expect("Failed to generate netlist")
}

/// The net a named instance's named pin is connected to, if any.
fn pin_net(netlist: &Netlist, inst_name: &str, pin_name: &str) -> Option<bhdl_netlist::NetId> {
    let (inst_id, _) = netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == inst_name)
        .unwrap_or_else(|| panic!("instance '{}' not found", inst_name));
    netlist
        .pin_instances
        .iter()
        .find(|(_, pi)| {
            pi.instance == inst_id
                && netlist.pins.get(pi.pin_def).map(|p| p.name == pin_name).unwrap_or(false)
        })
        .and_then(|(_, pi)| pi.net)
}

fn named_net(netlist: &Netlist, name: &str) -> bhdl_netlist::NetId {
    netlist
        .nets
        .iter()
        .find(|(_, n)| n.name.as_deref() == Some(name))
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("net '{}' not found", name))
}

#[tokio::test]
async fn bare_instance_chain_threads_series_regardless_of_declaration_order() {
    // The flow statement references R1/LED1 BEFORE their declarations —
    // the original reproducer left all four pins with net = None.
    let source = r#"
    entity RES() {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    entity LEDM() {
        pin A: signal in;
        pin K: signal in;
    }
    board SimpleLED {
        power VCC = 12V @ 1A;
        ground GND;

        VCC -> R1 -> LED1 -> GND;
        R1: RES();
        LED1: LEDM();
    }
    "#;
    let netlist = synthesize(source).await;

    let r1_1 = pin_net(&netlist, "R1", "1").expect("R1.1 unconnected");
    let r1_2 = pin_net(&netlist, "R1", "2").expect("R1.2 unconnected");
    let led_a = pin_net(&netlist, "LED1", "A").expect("LED1.A unconnected");
    let led_k = pin_net(&netlist, "LED1", "K").expect("LED1.K unconnected");

    // Series threading in pin-declaration order: entry pin joins the
    // incoming net, exit pin lands on the net the next endpoint joins.
    assert_eq!(r1_1, named_net(&netlist, "VCC"), "R1.1 must join VCC");
    assert_eq!(r1_2, led_a, "R1.2 and LED1.A must share the intermediate net");
    assert_ne!(r1_1, r1_2, "R1 must not be shorted");
    assert_eq!(led_k, named_net(&netlist, "GND"), "LED1.K must join GND");
}

#[tokio::test]
async fn bare_instance_chain_with_named_intermediate_net() {
    // `VCC -> R1 -> divider -> R2 -> GND` — bare instances thread,
    // `divider` names the mid net (test_synthesizer shape).
    let source = r#"
    entity RES() {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    board Divider {
        power VCC = 5V @ 1A;
        ground GND;

        VCC -> R1 -> divider -> R2 -> GND;
        R1: RES();
        R2: RES();
    }
    "#;
    let netlist = synthesize(source).await;

    let r1_2 = pin_net(&netlist, "R1", "2").expect("R1.2 unconnected");
    let r2_1 = pin_net(&netlist, "R2", "1").expect("R2.1 unconnected");
    assert_eq!(r1_2, r2_1, "R1.2 and R2.1 must meet on the divider net");
    assert_eq!(
        pin_net(&netlist, "R1", "1").unwrap(),
        named_net(&netlist, "VCC")
    );
    assert_eq!(
        pin_net(&netlist, "R2", "2").unwrap(),
        named_net(&netlist, "GND")
    );
}

#[tokio::test]
async fn polarized_terminal_aliases_resolve_pos_neg_pins() {
    // `.+` / `.-` on a part declaring `pos` / `neg` pins (the
    // ElectrolyticCap family from the test_7805_* boards).
    let source = r#"
    entity POLCAP() {
        pin pos: signal inout;
        pin neg: signal inout;
    }
    board CapBoard {
        power VCC = 5V @ 1A;
        ground GND;

        VCC -> c1: POLCAP().+;
        c1.- -> GND;
    }
    "#;
    let netlist = synthesize(source).await;

    assert_eq!(
        pin_net(&netlist, "c1", "pos").expect("c1.pos unconnected"),
        named_net(&netlist, "VCC")
    );
    assert_eq!(
        pin_net(&netlist, "c1", "neg").expect("c1.neg unconnected"),
        named_net(&netlist, "GND")
    );
}

#[tokio::test]
async fn bare_multi_pin_instance_is_refused_not_guessed() {
    // A bare reference to a 3-pin part has no unambiguous through-path:
    // it must hard-warn and leave the part alone — and must NOT mint a
    // net named after the instance.
    let source = r#"
    entity TRIODE() {
        pin A: signal inout;
        pin B: signal inout;
        pin C: signal inout;
    }
    board Bad {
        power VCC = 5V @ 1A;
        ground GND;

        VCC -> Q1 -> GND;
        Q1: TRIODE();
    }
    "#;
    let netlist = synthesize(source).await;

    for pin in ["A", "B", "C"] {
        assert!(
            pin_net(&netlist, "Q1", pin).is_none(),
            "Q1.{} must stay unconnected (ambiguous through-path)",
            pin
        );
    }
    assert!(
        !netlist
            .nets
            .iter()
            .any(|(_, n)| n.name.as_deref() == Some("Q1")),
        "no net may be minted with the instance's name"
    );
}
