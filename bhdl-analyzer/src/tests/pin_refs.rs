use super::common::*;

#[test]
fn analyze_pin_ref_ok() {
    let text = r#"
        component CompA {
            pins { pin p1: signal; }
        }
        board MyBoard {
            nets { net n1: signal; }
            components { component CompA C1; }
            connections { connect C1.p1 -> n1; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_pin_ref_undefined_instance() {
    let text = r#"
        component CompA {
            pins { pin p1: signal; }
        }
        board MyBoard {
            nets { net n1: signal; }
            // Instance C1 missing
            connections { connect C1.p1 -> n1; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_pin_ref_instance_not_instance() {
    let text = r#"
        board MyBoard {
            nets {
                net n1: signal;
                net NotAnInstance: signal;
            }
            components { component CompA C1; }
            connections { connect NotAnInstance.p1 -> n1; }
        }
        component CompA {
            pins { pin p1: signal; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_pin_ref_undefined_pin_in_component() {
    let text = r#"
        component CompA {
            // Pin p1 missing
        }
        board MyBoard {
            nets { net n1: signal; }
            components { component CompA C1; }
            connections { connect C1.p1 -> n1; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_pin_ref_symbol_not_a_pin() {
    let text = r#"
        component CompA_v2 {
             parameters { parameter P = 1; }
             pins { pin p1: signal; }
        }
        board MyBoard_v2 {
             nets { net n1: signal; }
             components { component CompA_v2 C2; }
             connections { connect C2.P -> n1; } // C2.P is a parameter, not a pin
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_pin_ref_in_module_ok() {
    let text = r#"
        module MyModule {
            ports { port p_ext: ExtIntf; }
            nets { net n_int: signal; }
            // Assuming connection syntax for interface ports is port_name.pin_name
            connections { connect n_int -> p_ext.member; }
        }
        // Interface definition uses PINS
        interface ExtIntf { pins { pin member: signal; } } // Changed port to pin
    "#;
    // Still expect semantic error for directionality later
    analyze_helper(text, false); // Parse should be OK now
}

#[test]
fn analyze_pin_ref_in_module_fail_undefined() {
    let text = r#"
        module MyModule {
            ports { port p_ext: ExtIntf; }
            connections { connect undefined_symbol -> p_ext.member; } // Connect to interface member
        }
        // Interface definition uses PINS
        interface ExtIntf { pins { pin member: signal; } } // Changed port to pin
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_pin_ref_in_module_fail_not_pin_or_net() {
    let text = r#"
        module MyModule {
            ports { port p_ext: ExtIntf; }
            parameters { parameter NotAPinOrNet = 5; }
            connections { connect NotAPinOrNet -> p_ext.member; } // Connect to interface member
        }
        // Interface definition uses PINS
         interface ExtIntf { pins { pin member: signal; } } // Changed port to pin
    "#;
    analyze_helper(text, true);
}