//! v0.8 swizzle-via-generate — end-to-end synth test.
//!
//! Demonstrates that board-level DDR-style swizzle (byte-lane swap +
//! intra-byte bit permutation) is expressible with the generalised
//! generate-loop primitive alone:
//!
//!   - literal-list iteration: `for i in [2, 3, 0, 1]`
//!   - paired (idx, val) destructuring: `for (j, i) in [...]`
//!   - top-level unrolling (outside parametric templates)
//!
//! No swizzle-specific syntax needed. The permutation table IS the
//! list literal.

use bhdl_synthesizer::parametric_resolver::preprocess;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

const SOURCE: &str = r#"
interface DiffPair {
    signal P: inout;
    signal N: inout;
}

interface ByteLane {
    signal DQ0: inout;
    signal DQ1: inout;
    signal DQ2: inout;
    signal DQ3: inout;
    interface DiffPair DQS;
}

interface DDR4Mini {
    interface DiffPair CK;
    interface ByteLane lane0;
    interface ByteLane lane1;
    interface ByteLane lane2;
    interface ByteLane lane3;
}

entity MemController { interface DDR4Mini ddr; }
entity MemoryChip    { interface DDR4Mini ddr; }

board TestBoard {
    power VCC = 1.2V @ 1A;
    ground GND;

    mc:  MemController();
    mem: MemoryChip();

    // Byte swizzle: mc.lane0 → mem.lane2, mc.lane1 → mem.lane3,
    //               mc.lane2 → mem.lane0, mc.lane3 → mem.lane1.
    // The list literal [2, 3, 0, 1] *is* the permutation table.
    generate for (j, i) in [2, 3, 0, 1] {
        mc.lane<j> -> mem.lane<i>;
    }

    // Bit swizzle within lane0: DQ0..DQ3 of mc map to DQ2, DQ0, DQ3, DQ1 of mem.
    generate for (j, i) in [2, 0, 3, 1] {
        mc.lane0.DQ<j> -> mem.lane0.DQ<i>;
    }
}
"#;

fn main() {
    let rewritten = match preprocess(SOURCE) {
        Ok(s) => s,
        Err(e) => fail(&format!("preprocess error: {}", e)),
    };

    println!("=== rewritten source ===\n{}=== /rewritten ===", rewritten);

    // After preprocessing, the generate blocks should be gone and
    // the bytewise + bitwise swaps should appear as explicit
    // connection statements.
    if rewritten.contains("generate for") {
        fail("rewritten source still contains an unexpanded `generate for` block");
    }

    let want_byte_swizzle = [
        "mc.lane0 -> mem.lane2;",
        "mc.lane1 -> mem.lane3;",
        "mc.lane2 -> mem.lane0;",
        "mc.lane3 -> mem.lane1;",
    ];
    for line in want_byte_swizzle.iter() {
        if !rewritten.contains(line) {
            fail(&format!("byte swizzle missing connection `{}`", line));
        }
        println!("✓ byte swizzle: {}", line);
    }

    let want_bit_swizzle = [
        "mc.lane0.DQ0 -> mem.lane0.DQ2;",
        "mc.lane0.DQ1 -> mem.lane0.DQ0;",
        "mc.lane0.DQ2 -> mem.lane0.DQ3;",
        "mc.lane0.DQ3 -> mem.lane0.DQ1;",
    ];
    for line in want_bit_swizzle.iter() {
        if !rewritten.contains(line) {
            fail(&format!("bit swizzle missing connection `{}`", line));
        }
        println!("✓ bit swizzle: {}", line);
    }

    // Sanity: the rewritten source should still parse cleanly.
    let pr = bhdl_parser::parse(&rewritten);
    if !pr.errors().is_empty() {
        eprintln!("✗ rewritten source has parse errors:");
        for e in pr.errors().iter().take(10) {
            eprintln!("    {:?}", e);
        }
        std::process::exit(1);
    }
    println!("✓ rewritten source parses cleanly");

    println!("\nv0.8 swizzle via generate-with-list (no swizzle-specific machinery): PASS");
}
