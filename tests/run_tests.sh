#!/bin/bash
# BHDL Test Runner Script

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test directories
TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CIRCUITS_DIR="$TEST_DIR/circuits"
OUTPUTS_DIR="$TEST_DIR/outputs"

echo -e "${GREEN}BHDL Test Runner${NC}"
echo "=================="

# Function to run a test binary
run_test() {
    local crate=$1
    local bin=$2
    local circuit=${3:-}
    
    echo -e "\n${YELLOW}Running $crate::$bin${NC}"
    
    if [ -n "$circuit" ]; then
        # Run with circuit file
        cargo run -p $crate --bin $bin -- "$CIRCUITS_DIR/$circuit" 2>&1
    else
        # Run without arguments
        cargo run -p $crate --bin $bin 2>&1
    fi
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ $bin passed${NC}"
    else
        echo -e "${RED}✗ $bin failed${NC}"
    fi
}

# Parse command line arguments
case "$1" in
    "analyzer")
        echo "Running analyzer tests..."
        run_test bhdl-analyzer test_7805_analyzer "realistic/test_7805_regulator.bhdl"
        run_test bhdl-analyzer test_stdlib_inference
        ;;
    
    "synthesizer")
        echo "Running synthesizer tests..."
        run_test bhdl-synthesizer test_pipeline_7805
        run_test bhdl-synthesizer test_7805_realistic
        run_test bhdl-synthesizer test_net_assignment
        ;;
    
    "visualizer")
        echo "Running visualizer tests..."
        echo "Note: SVG outputs will be in $OUTPUTS_DIR/svg/"
        run_test bhdl-visualizer test_semantic_visualizer
        run_test bhdl-visualizer test_semantic_real
        ;;
    
    "parser")
        echo "Running parser tests..."
        run_test bhdl-parser test_v2_parser
        run_test bhdl-parser test_v2_comprehensive
        ;;
    
    "e2e"|"end-to-end")
        echo "Running end-to-end tests..."
        run_test bhdl-synthesizer end_to_end_test
        ;;
    
    "all")
        echo "Running all test suites..."
        $0 parser
        $0 analyzer
        $0 synthesizer
        $0 visualizer
        $0 e2e
        ;;
    
    *)
        echo "Usage: $0 {parser|analyzer|synthesizer|visualizer|e2e|all}"
        echo ""
        echo "Examples:"
        echo "  $0 analyzer     # Run analyzer tests"
        echo "  $0 e2e          # Run end-to-end pipeline test"
        echo "  $0 all          # Run all test suites"
        exit 1
        ;;
esac

echo -e "\n${GREEN}Test run complete${NC}"