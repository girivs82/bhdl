#!/bin/bash

# MAESTRO Complete Experiment Runner
# This script reproduces all results from the paper

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}MAESTRO Experiment Runner${NC}"
echo "================================"

# Check prerequisites
echo -e "\n${YELLOW}Checking prerequisites...${NC}"
command -v cargo >/dev/null 2>&1 || { echo -e "${RED}Rust/Cargo not found. Please install Rust.${NC}" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo -e "${RED}Python 3 not found. Please install Python 3.8+.${NC}" >&2; exit 1; }

# Build release version
echo -e "\n${YELLOW}Building MAESTRO (release mode)...${NC}"
cd ..
cargo build --release

# Create output directories
echo -e "\n${YELLOW}Creating output directories...${NC}"
mkdir -p ../Raw_Data
mkdir -p ../outputs/figures
mkdir -p ../outputs/logs

# Circuit categories
declare -a categories=("series" "parallel" "power" "amplifier" "bridge" "protection")

# Run experiments for each category
echo -e "\n${GREEN}Running experiments...${NC}"
total_circuits=0
successful=0

for category in "${categories[@]}"; do
    echo -e "\n${YELLOW}Testing $category circuits...${NC}"
    
    for circuit in circuits/$category/*.net; do
        if [ -f "$circuit" ]; then
            circuit_name=$(basename "$circuit" .net)
            echo -n "  $circuit_name: "
            
            # Run all solvers
            output_file="../Raw_Data/${circuit_name}_results.csv"
            log_file="../outputs/logs/${circuit_name}.log"
            
            # Header for CSV
            echo "solver,converged,iterations,time_ms,residual,strategy" > "$output_file"
            
            # Test each solver
            for solver in "newton" "glacier" "maestro" "maestro_glacier"; do
                timeout 300s cargo run --release --bin test_solver -- \
                    --circuit "$circuit" \
                    --solver "$solver" \
                    --output "$output_file" \
                    >> "$log_file" 2>&1
                
                if [ $? -eq 0 ]; then
                    echo -n "✓"
                else
                    echo -n "✗"
                fi
            done
            
            echo ""
            ((total_circuits++))
            
            # Check if MAESTRO succeeded
            if grep -q "maestro,true" "$output_file"; then
                ((successful++))
            fi
        fi
    done
done

# Aggregate results
echo -e "\n${YELLOW}Aggregating results...${NC}"
python3 ../analysis/aggregate_results.py \
    --input ../Raw_Data \
    --output ../Raw_Data/maestro_results.csv

# Generate statistics
echo -e "\n${YELLOW}Computing statistics...${NC}"
python3 ../analysis/compute_statistics.py \
    --input ../Raw_Data/maestro_results.csv \
    --output ../outputs/statistics.json

# Generate visualizations
echo -e "\n${YELLOW}Generating visualizations...${NC}"
python3 ../visualization/generate_all_plots.py \
    --data ../Raw_Data/maestro_results.csv \
    --output ../outputs/figures

# Summary
echo -e "\n${GREEN}Experiment Summary${NC}"
echo "================================"
echo "Total circuits tested: $total_circuits"
echo "MAESTRO successful: $successful"
success_rate=$((successful * 100 / total_circuits))
echo "Success rate: ${success_rate}%"

# Performance summary
echo -e "\n${YELLOW}Performance Summary:${NC}"
python3 ../analysis/performance_summary.py \
    --input ../Raw_Data/maestro_results.csv

# Validation
echo -e "\n${YELLOW}Validating results...${NC}"
python3 ../analysis/validate_solutions.py \
    --circuits circuits/ \
    --solutions ../Raw_Data/

echo -e "\n${GREEN}All experiments completed!${NC}"
echo "Results saved to:"
echo "  - Raw data: ../Raw_Data/"
echo "  - Figures: ../outputs/figures/"
echo "  - Logs: ../outputs/logs/"
echo "  - Statistics: ../outputs/statistics.json"

# Generate final report
echo -e "\n${YELLOW}Generating final report...${NC}"
python3 ../analysis/generate_report.py \
    --template ../templates/report_template.md \
    --data ../outputs/statistics.json \
    --output ../MAESTRO_Results_Report.pdf

echo -e "\n${GREEN}Done! Report saved to ../MAESTRO_Results_Report.pdf${NC}"