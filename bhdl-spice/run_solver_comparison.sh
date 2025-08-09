#!/bin/bash
# Run the solver comparison metrics tool

echo "Building solver comparison metrics tool..."
cargo build -p bhdl-spice --bin solver_comparison_metrics --release

if [ $? -eq 0 ]; then
    echo "Running solver comparisons..."
    ./target/release/solver_comparison_metrics
    
    if [ -f solver_comparison_report.md ]; then
        echo ""
        echo "Report generated successfully: solver_comparison_report.md"
        echo "Preview of results:"
        echo "=================="
        head -n 50 solver_comparison_report.md
    fi
else
    echo "Build failed. Please check for compilation errors."
fi