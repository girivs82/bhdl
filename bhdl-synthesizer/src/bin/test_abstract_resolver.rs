//! v0.9b: end-to-end test of abstract-entity resolution with
//! alias-aware pin maps.
//!
//! Each family entry declares its own `pin_map` mapping abstract
//! aliases (lowercase by convention) to the concrete entity's
//! pin names. The abstract layer carries SKU-specific naming
//! differences — DIP-28 uses `VCC` for its supply pin, QFN-32
//! uses `VCC1`/`VCC2`; the abstract layer exposes a single `vcc`
//! alias that maps to whichever pin the chosen SKU actually has.
//!
//! Two boards:
//!
//!   A: uses only aliases available on the DIP-28's pin_map →
//!      resolver picks DIP-28 (first in family).
//!
//!   B: uses `adc7` (only in the QFN's pin_map) → resolver skips
//!      DIP-28 and picks QFN-32.
//!
//! Both boards write the SAME `mcu.vcc` / `mcu.gnd` lines; the
//! abstract layer hides the SKU-specific VCC1 vs VCC distinction.

use bhdl_parser::parse;
use bhdl_synthesizer::abstract_resolver::preprocess;
use anyhow::Result;

const BOARD_A_DIP_FITS: &str = r#"
import { ATmega328P_DIP28 } from "bhdl-stdlib/actives/atmega328p.bhdl";
import { ATmega328P_QFN32 } from "bhdl-stdlib/actives/atmega328p.bhdl";

abstract entity ATmega328P {
    // Abstract port list — the surface a board author reads to know
    // what's available. Each family entry's pin_map maps these ports
    // to that SKU's concrete pin names; SKUs that don't expose a port
    // simply omit it from their pin_map.
    pin vcc:   signal inout;
    pin avcc:  signal inout;
    pin gnd:   signal inout;
    pin agnd:  signal inout;
    pin adc0:  signal inout;
    pin adc1:  signal inout;
    pin adc2:  signal inout;
    pin adc3:  signal inout;
    pin adc4:  signal inout;
    pin adc5:  signal inout;
    pin adc6:  signal inout;   // QFN-only
    pin adc7:  signal inout;   // QFN-only
    pin reset: signal inout;

    family {
        ATmega328P_DIP28 {
            vcc  = VCC;
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND2;
            adc0 = PC0;  adc1 = PC1;  adc2 = PC2;
            adc3 = PC3;  adc4 = PC4;  adc5 = PC5;
            reset = PC6;
            // No adc6 / adc7 — DIP-28 doesn't bring them out.
        };
        ATmega328P_QFN32 {
            vcc  = VCC1;
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND3;
            adc0 = PC0;  adc1 = PC1;  adc2 = PC2;
            adc3 = PC3;  adc4 = PC4;  adc5 = PC5;
            adc6 = ADC6;
            adc7 = ADC7;
            reset = PC6;
        };
    }
}

board ATmega328P_Abstract_DIP_Fits {
    power VCC = 5V @ 200mA;
    ground GND;

    mcu: ATmega328P();
    @VCC -> mcu.vcc;
    @VCC -> mcu.avcc;
    mcu.gnd -> @GND;
    mcu.agnd -> @GND;
    mcu.adc0 -> @ADC_IN;
}
"#;

const BOARD_B_NEEDS_QFN: &str = r#"
import { ATmega328P_DIP28 } from "bhdl-stdlib/actives/atmega328p.bhdl";
import { ATmega328P_QFN32 } from "bhdl-stdlib/actives/atmega328p.bhdl";

abstract entity ATmega328P {
    family {
        ATmega328P_DIP28 {
            vcc  = VCC;
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND2;
            adc0 = PC0; adc1 = PC1; adc2 = PC2;
            adc3 = PC3; adc4 = PC4; adc5 = PC5;
            reset = PC6;
        };
        ATmega328P_QFN32 {
            vcc  = VCC1;
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND3;
            adc0 = PC0; adc1 = PC1; adc2 = PC2;
            adc3 = PC3; adc4 = PC4; adc5 = PC5;
            adc6 = ADC6; adc7 = ADC7;
            reset = PC6;
        };
    }
}

board ATmega328P_Abstract_QFN_Needed {
    power VCC = 5V @ 200mA;
    ground GND;

    mcu: ATmega328P();
    @VCC -> mcu.vcc;
    @VCC -> mcu.avcc;
    mcu.gnd -> @GND;
    mcu.agnd -> @GND;
    mcu.adc7 -> @THERMISTOR_IN;   // QFN-only alias — forces SKU choice
}
"#;

fn main() -> Result<()> {
    println!("=== Board A: DIP-28 sufficient (no QFN-only aliases used) ===");
    let rewritten_a = preprocess(BOARD_A_DIP_FITS)?;
    if !rewritten_a.contains("mcu: ATmega328P_DIP28(") {
        eprintln!("✗ rewritten source missing `mcu: ATmega328P_DIP28(`");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_a);
        std::process::exit(1);
    }
    // Pin-alias rewrites: mcu.vcc → mcu.VCC, mcu.gnd → mcu.GND1, etc.
    if !rewritten_a.contains("mcu.VCC") || !rewritten_a.contains("mcu.GND1")
        || !rewritten_a.contains("mcu.GND2") || !rewritten_a.contains("mcu.PC0") {
        eprintln!("✗ rewritten Board A missing expected concrete pin refs");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_a);
        std::process::exit(1);
    }
    if rewritten_a.contains("mcu.vcc") || rewritten_a.contains("mcu.adc0") {
        eprintln!("✗ rewritten Board A still contains abstract aliases");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_a);
        std::process::exit(1);
    }
    if rewritten_a.contains("abstract entity") {
        eprintln!("✗ rewritten Board A still contains `abstract entity` decl");
        std::process::exit(1);
    }
    let pr = parse(&rewritten_a);
    if !pr.errors().is_empty() {
        eprintln!("✗ Board A rewritten source has parse errors:");
        for e in pr.errors().iter().take(3) { eprintln!("    {:?}", e); }
        std::process::exit(1);
    }
    println!("✓ Board A → DIP-28; aliases rewritten to concrete pin names; parses clean");

    println!("\n=== Board B: needs adc7 → QFN-32 (and vcc maps to VCC1) ===");
    let rewritten_b = preprocess(BOARD_B_NEEDS_QFN)?;
    if !rewritten_b.contains("mcu: ATmega328P_QFN32(") {
        eprintln!("✗ Board B should have picked QFN-32 (adc7 forces it)");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_b);
        std::process::exit(1);
    }
    if rewritten_b.contains("ATmega328P_DIP28(") {
        eprintln!("✗ Board B picked DIP-28 incorrectly (no adc7 in DIP map)");
        std::process::exit(1);
    }
    // mcu.vcc → mcu.VCC1 (QFN's mapping, *not* DIP's VCC)
    if !rewritten_b.contains("mcu.VCC1") {
        eprintln!("✗ Board B's mcu.vcc should have rewritten to mcu.VCC1 \
                   (QFN's pin_map says vcc=VCC1)");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_b);
        std::process::exit(1);
    }
    if rewritten_b.contains("mcu.VCC ") || rewritten_b.contains("mcu.VCC\n")
        || rewritten_b.contains("mcu.VCC;") || rewritten_b.contains("mcu.VCC,") {
        eprintln!("✗ Board B contains mcu.VCC (DIP-28's name) but QFN-32 \
                   uses VCC1; alias rewrite chose wrong pin_map");
        std::process::exit(1);
    }
    // mcu.adc7 → mcu.ADC7
    if !rewritten_b.contains("mcu.ADC7") {
        eprintln!("✗ Board B's mcu.adc7 should have rewritten to mcu.ADC7");
        eprintln!("--- rewritten ---\n{}\n---", rewritten_b);
        std::process::exit(1);
    }
    let pr = parse(&rewritten_b);
    if !pr.errors().is_empty() {
        eprintln!("✗ Board B rewritten source has parse errors:");
        for e in pr.errors().iter().take(3) { eprintln!("    {:?}", e); }
        std::process::exit(1);
    }
    println!("✓ Board B → QFN-32; vcc→VCC1 (SKU-specific); adc7→ADC7; parses clean");

    // Board C: deliberately references an abstract port that isn't
    // declared. The resolver should catch this with a port-name
    // error before SKU resolution, naming the *abstract* entity (not
    // a concrete SKU) so the user sees the diagnostic relative to
    // what they wrote.
    let bad_board = r#"
abstract entity ATmega328P {
    pin vcc:  signal inout;
    pin gnd:  signal inout;
    pin adc0: signal inout;
    family {
        ATmega328P_DIP28 { vcc = VCC; gnd = GND1; adc0 = PC0; };
    }
}

board Misuse {
    mcu: ATmega328P();
    mcu.adc9 -> @SOMEWHERE;   // ← no such port on the abstract entity
}
"#;
    println!("\n=== Board C: undeclared port → error ===");
    match preprocess(bad_board) {
        Ok(_) => {
            eprintln!("✗ preprocess should have errored on undeclared port 'adc9'");
            std::process::exit(1);
        }
        Err(e) => {
            let msg = format!("{}", e);
            if !msg.contains("adc9") || !msg.contains("ATmega328P") {
                eprintln!("✗ error message doesn't name the bad port and abstract \
                           entity clearly: {}", msg);
                std::process::exit(1);
            }
            println!("✓ caught with clear diagnostic: {}", msg);
        }
    }

    println!("\nv0.9b abstract-entity resolution (alias-aware, port-validated): PASS");
    Ok(())
}
