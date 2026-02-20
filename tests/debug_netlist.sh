#!/bin/bash

echo "=== Netlist Debugging Tool ==="
echo ""

# Run the CLI with JSON output to see the netlist structure
RUST_LOG=bhdl_synthesizer=info cargo run -p bhdl-cli --bin bhdl-cli \
    tests/circuits/simple/test_intent_simple_demo.bhdl \
    analyze 2>&1 | grep -E "(instances|nets|connections|pins)" | head -30

echo ""
echo "=== Component Pin Analysis ==="

# Check what pins are in the component symbols from the database
echo "Checking KiCad database for component pins..."
sqlite3 components.db "SELECT name, symbol_name FROM components WHERE name IN ('R', 'C', 'D_TVS', 'LED_Dual_Bidirectional') LIMIT 5;" 2>/dev/null || echo "Database not found"

echo ""
echo "=== Layout Debug (Pin Matching) ==="

# Run with maximum debug logging for layout
RUST_LOG=bhdl_visualizer::layout=trace cargo run -p bhdl-cli --bin bhdl-cli \
    tests/circuits/simple/test_intent_simple_demo.bhdl \
    visualize -o /dev/null 2>&1 | grep -E "(Connection:|Found component|Pin from netlist|Pin .*not found|Component pins available)" | head -50
