use super::common::*;

// Note: Pin directionality is now SEMANTIC, not syntactic.
// The analyzer must determine if connections are valid based on pin usage context.
// Port directionality is determined by the Interface definition.
// Components cannot contain nets/connections, using MODULES for these tests.

#[test]
fn analyze_assign_to_input_pin_fail() {
    let text = r#"
        // Assigning to an input-like pin in a module context is likely an error.
        module MyModule {
            // Module pins are its external interface - use ports + interface
            ports { port p_in: InIntf; }
            nets { net n_int: signal; }
            // Assigning to an interface port member?
            connections { assign p_in.member = n_int; } // Assuming InIntf has 'member'
        }
        interface InIntf { pins { pin member: signal; } }
    "#;
    // Semantic check should catch assigning to input-like port pin.
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_out_to_out_pin_fail() {
    let text = r#"
        module MyModule {
            // Define two pins, implicitly output by usage (driving nets)
            // Need to use ports/interfaces for module external pins
            ports { port p_out1: OutIntf; port p_out2: OutIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 connect p_out1.member -> n1;
                 connect p_out2.member -> n2;
                 connect p_out1.member -> p_out2.member; // Connecting two outputs (semantically)
            }
        }
        interface OutIntf { pins { pin member: signal; } }
    "#;
    // Semantic check needed
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_in_to_in_pin_fail() {
    let text = r#"
         module MyModule {
            // Define two pins, implicitly input by usage (driven by nets)
            ports { port p_in1: InIntf; port p_in2: InIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 connect n1 -> p_in1.member;
                 connect n2 -> p_in2.member;
                 connect p_in1.member -> p_in2.member; // Connecting two inputs (semantically)
            }
        }
         interface InIntf { pins { pin member: signal; } }
    "#;
    // Semantic check needed
    analyze_helper(text, true);
}


#[test]
fn analyze_conn_in_to_inout_pin_fail() {
     let text = r#"
         module MyModule {
            ports { port p_in: InIntf; port p_io: InOutIntf; }
            nets { net n1: signal; }
            connections {
                 connect n1 -> p_in.member; // p_in is input
                 // p_io used as inout implicitly
                 connect n1 -> p_io.member;
                 connect p_io.member -> n1;
                 // Attempt connect input -> inout
                 connect p_in.member -> p_io.member;
            }
        }
        interface InIntf { pins { pin member: signal; } }
        interface InOutIntf { pins { pin member: signal; } } // Assume inout by usage
    "#;
    // Semantic check needed
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_inout_to_in_pin_fail() {
    let text = r#"
         module MyModule {
            ports { port p_in: InIntf; port p_io: InOutIntf; }
            nets { net n1: signal; }
            connections {
                 connect n1 -> p_in.member; // p_in is input
                 // p_io used as inout implicitly
                 connect n1 -> p_io.member;
                 connect p_io.member -> n1;
                 // Attempt connect inout -> input
                 connect p_io.member -> p_in.member;
            }
        }
        interface InIntf { pins { pin member: signal; } }
        interface InOutIntf { pins { pin member: signal; } } // Assume inout by usage
    "#;
    // Semantic check needed
    analyze_helper(text, true);
}

// --- OK Scenarios (Semantic Checks) --- 
// These assume the analyzer correctly infers/validates directions based on usage.

#[test]
fn analyze_conn_out_to_in_pin_ok() {
    let text = r#"
        module MyModule {
            ports { port p_out: OutIntf; port p_in: InIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 connect p_out.member -> n1; // p_out is output
                 connect n2 -> p_in.member;  // p_in is input
                 connect p_out.member -> p_in.member; // OK: output to input
            }
        }
        interface OutIntf { pins { pin member: signal; } }
        interface InIntf { pins { pin member: signal; } }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_conn_out_to_inout_pin_ok() {
     let text = r#"
         module MyModule {
            ports { port p_out: OutIntf; port p_io: InOutIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 connect p_out.member -> n1; // p_out is output
                 // p_io is inout
                 connect n2 -> p_io.member;
                 connect p_io.member -> n2;
                 connect p_out.member -> p_io.member; // OK: output to inout
            }
        }
         interface OutIntf { pins { pin member: signal; } }
         interface InOutIntf { pins { pin member: signal; } } // Assume inout by usage
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_conn_inout_to_out_pin_ok() {
    let text = r#"
         module MyModule {
            ports { port p_out: OutIntf; port p_io: InOutIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 connect p_out.member -> n1; // p_out is output
                 // p_io is inout
                 connect n2 -> p_io.member;
                 connect p_io.member -> n2;
                 connect p_io.member -> p_out.member; // OK: inout to output
            }
        }
         interface OutIntf { pins { pin member: signal; } }
         interface InOutIntf { pins { pin member: signal; } } // Assume inout by usage
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_conn_inout_to_inout_pin_ok() {
    let text = r#"
         module MyModule {
            ports { port p_io1: InOutIntf; port p_io2: InOutIntf; }
            nets { net n1: signal; net n2: signal; }
            connections {
                 // p_io1 is inout
                 connect n1 -> p_io1.member;
                 connect p_io1.member -> n1;
                 // p_io2 is inout
                 connect n2 -> p_io2.member;
                 connect p_io2.member -> n2;
                 connect p_io1.member -> p_io2.member; // OK: inout to inout
            }
        }
         interface InOutIntf { pins { pin member: signal; } } // Assume inout by usage
    "#;
    analyze_helper(text, false);
}

 #[test]
fn analyze_conn_net_to_in_pin_ok() {
    let text = r#"
        module MyModule {
             ports { port p_in: InIntf; }
             nets { net n_int: signal; }
            connections {
                 connect n_int -> p_in.member; // OK: Net driving input port pin
            }
        }
         interface InIntf { pins { pin member: signal; } }
    "#;
    analyze_helper(text, false);
} 