#!/bin/bash
# Run the MAESTRO solver comparison tool

echo "Building MAESTRO comparison metrics tool..."
cargo build -p bhdl-spice --bin maestro_comparison_metrics --release

if [ $? -eq 0 ]; then
    echo "Running MAESTRO solver comparisons..."
    echo "This will test 52 circuits across 6 categories with 4 solver configurations."
    echo "Expected runtime: 5-10 minutes"
    echo ""
    
    ./target/release/maestro_comparison_metrics
    
    if [ -f maestro_comparison_report.md ]; then
        echo ""
        echo "Report generated successfully: maestro_comparison_report.md"
        echo ""
        echo "Preview of results:"
        echo "=================="
        head -n 60 maestro_comparison_report.md
        echo ""
        echo "... (see full report for details)"
    fi
else
    echo "Build failed. Please check for compilation errors."
fi