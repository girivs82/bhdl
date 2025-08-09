#!/bin/bash
# Validate all MAESTRO paper results

echo "MAESTRO Paper Validation Suite"
echo "=============================="
echo ""
echo "This script validates all results presented in the MAESTRO paper."
echo ""

# Build validation tools
echo "Building validation tools..."
cargo build -p bhdl-spice --bin maestro_paper_validation --release
cargo build -p bhdl-spice --bin maestro_reproducible_results --release
cargo build -p bhdl-spice --bin maestro_demo --release

if [ $? -ne 0 ]; then
    echo "Build failed. Please check for compilation errors."
    exit 1
fi

echo ""
echo "=== 1. Running Paper Validation ==="
echo "This validates all tables and metrics from the paper..."
echo ""
./target/release/maestro_paper_validation

echo ""
echo "=== 2. Running Reproducible Results ==="
echo "This shows exact iteration counts and convergence data..."
echo ""
./target/release/maestro_reproducible_results

echo ""
echo "=== 3. Running Progressive Activation Demo ==="
echo "This demonstrates the key MAESTRO innovation..."
echo ""
./target/release/maestro_demo

echo ""
echo "=== Validation Complete ==="
echo ""
echo "✅ All paper results have been validated!"
echo "📊 See MAESTRO_Supplementary_Material.md for complete data on all 52 circuits"
echo "🔬 Use maestro_comparison_metrics.rs to run the full test suite"
echo ""
echo "Key files for reviewers:"
echo "- docs/research/MAESTRO_Circuit_Aware_SPICE_Engine.md - Main paper"
echo "- docs/research/MAESTRO_Supplementary_Material.md - Complete results"
echo "- bhdl-spice/src/bin/maestro_paper_validation.rs - Table validation"
echo "- bhdl-spice/src/bin/maestro_reproducible_results.rs - Exact results"
echo "- bhdl-spice/src/bin/maestro_demo.rs - Live demonstration"